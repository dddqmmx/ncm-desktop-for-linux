import { Song, SongDetailResult } from '@renderer/types/songDetail'
import { defineStore } from 'pinia'
import { ref, watch, computed } from 'vue'
import { useUserStore } from './userStore'
import { SongUrl, type SoundQualityType } from '@renderer/types/song'
import { useConfigStore } from './configStore'

// 播放模式定义
export type PlayMode =  'loop' | 'random' | 'single'

export interface CurrentSong {
  id: number
  name: string
  artist: string
  cover: string
  duration: number
}

export const usePlayerStore = defineStore('player', () => {
  // --- 状态 (State) ---
  const currentSong = ref<CurrentSong | null>(null)
  const currentTime = ref(Number(localStorage.getItem('currentTime') || 0))
  const currentSongId = ref<number | null>(Number(localStorage.getItem('currentSongId')) || null)
  const isPlaying = ref(false)
  const isFullScreen = ref(false)
  const isHistorySong = ref(true)
  const isSeeking = ref(false)

  // --- 播放列表相关状态 ---
  const playlist = ref<CurrentSong[]>(JSON.parse(localStorage.getItem('playlist') || '[]'))
  const playMode = ref<PlayMode>((localStorage.getItem('playMode') as PlayMode) || 'loop')

  const userStore = useUserStore()
  const configStore = useConfigStore()
  let progressTimer: ReturnType<typeof setInterval> | null = null

  // --- 计算属性 (Getters) ---
  const duration = computed(() => currentSong.value?.duration || 0)
  const progressPercent = computed(() => {
    if (duration.value <= 0) return 0
    return ((currentTime.value % duration.value) / duration.value) * 100
  })

  // 获取当前歌曲在列表中的索引
  const currentIndex = computed(() => {
    return playlist.value.findIndex(s => s.id === currentSongId.value)
  })

  // --- 音质映射 ---
  const qualityMap: Partial<Record<SoundQualityType, { key: keyof Song; rate: number }>> = {
    standard: { key: 'l', rate: 128000 },
    exhigh: { key: 'h', rate: 320000 },
    lossless: { key: 'sq', rate: 964935 },
    hires: { key: 'hr', rate: 192000 },
    jyeffect: { key: 'jyeffect', rate: 999000 },
  }

  // 降级顺序（从高到低）
  const downgradeOrder: SoundQualityType[] = ['hires', 'lossless', 'exhigh', 'standard']

  // --- 私有辅助函数 ---
  const getSongDetail = async (id: number): Promise<Song | undefined> => {
    const res = await window.api.song_detail({ ids: [id] }) as { body?: SongDetailResult }
    console.log(res)
    return res.body?.songs?.[0]
  }

  // 获取支持的音质列表
  const getAvailableQualities = (song: Song): SoundQualityType[] => {
    const available: SoundQualityType[] = []
    for (const [name, value] of Object.entries(qualityMap)) {
      if (!value) continue
      if (song[value.key as keyof Song]) available.push(name as SoundQualityType)
    }
    return available
  }

  // 根据目标音质选择可用音质（不可用则降级）
  const getPlayableQuality = (song: Song, targetQuality: SoundQualityType): SoundQualityType | null => {
    const available = getAvailableQualities(song)
    if (available.includes(targetQuality)) return targetQuality

    const targetIndex = downgradeOrder.indexOf(targetQuality)
    for (let i = targetIndex + 1; i < downgradeOrder.length; i++) {
      if (available.includes(downgradeOrder[i])) return downgradeOrder[i]
    }

    // 如果都没有，则返回最低可用
    return available[available.length - 1] || null
  }

  // 获取歌曲播放 URL（自动降级音质）
  const getSongUrl = async (
    song_id: number,
    targetQuality: SoundQualityType = 'hires',
  ): Promise<string> => {
    const song = await getSongDetail(song_id)
    if (!song) return ''

    const playableQuality = getPlayableQuality(song, targetQuality) || 'standard'

    const res = (await window.api.song_url({
      id: song_id,
      level: playableQuality,
      cookie: userStore.cookie,
    })) as { body?: { data?: SongUrl[] } }

    return res.body?.data?.[0].url ?? ''
  }


  const syncProgress = async () => {
    if (isSeeking.value) return
    try {
      const progressMs = await window.api.get_progress();
      if (progressMs !== undefined && progressMs !== null) {
        currentTime.value = progressMs;
      }
    } catch (error) {
      console.error('同步进度失败:', error);
    }
  }

  const startTimer = () => {
    if (progressTimer) return
    progressTimer = setInterval(syncProgress, 1000)
  }

  const stopTimer = () => {
    if (progressTimer) {
      clearInterval(progressTimer)
      progressTimer = null
    }
  }

  // --- 核心操作 (Actions) ---

const playNext = async (isAuto = false) => {
  if (playlist.value.length === 0) return

  // 1. 处理单曲循环
  if (isAuto && playMode.value === 'single' && currentSongId.value) {
    // 强制重新播放当前歌曲
    await playMusic(currentSongId.value, 0, true)
    return
  }

  // 2. 计算下一首的索引
  let nextIndex = 0
  if (playMode.value === 'random') {
    nextIndex = Math.floor(Math.random() * playlist.value.length)
  } else {
    nextIndex = currentIndex.value + 1
    // 如果是最后一首，循环回第一首
    if (nextIndex >= playlist.value.length) {
      nextIndex = 0
    }
  }

  const nextSong = playlist.value[nextIndex]
  if (nextSong) {
    await playMusic(nextSong.id)
  }
}

  // 2. 播放上一首
  const playPrev = async () => {
    if (playlist.value.length === 0) return

    let prevIndex = 0
    if (playMode.value === 'random') {
      prevIndex = Math.floor(Math.random() * playlist.value.length)
    } else {
      prevIndex = currentIndex.value - 1
      if (prevIndex < 0) {
        prevIndex = playlist.value.length - 1
      }
    }

    const prevSong = playlist.value[prevIndex]
    await playMusic(prevSong.id)
  }

  // 在 playToken 定义旁加上 switching
  let playToken = 0
  const isSwitching = ref(false)

  const waitForEnd = async (songId: number, token: number) => {
    try {
      await window.api.wait_finished()
      // 丢弃在切歌期间或过期 token 的结束事件
      if (token !== playToken) return
      if (isSwitching.value) return
      if (currentSongId.value !== songId) return

      isPlaying.value = false
      stopTimer()
      currentTime.value = duration.value
      await playNext(true)
    } catch { }
  }


  /**
   * 播放整份列表
   * @param list 歌曲列表
   * @param startIndex 从哪一首开始播放，默认为第0首
   */
  const playAll = async (list: CurrentSong[], startIndex = 0) => {
    if (list.length === 0) return

    // 1. 替换整个播放列表
    playlist.value = [...list]

    // 2. 播放指定位置的歌曲
    const targetSong = list[startIndex]
    await playMusic(targetSong.id)
  }

/**
 * @param song_id 歌曲ID
 * @param startTime 起始时间
 * @param forceRestart 是否强制重新加载资源（用于单曲循环）
 */
const playMusic = async (song_id: number, startTime: number = 0, forceRestart: boolean = false) => {
  if (isSwitching.value) return

  // 只有在【非强制重启】且【歌曲相同】且【正在播放】时才拦截
  if (!forceRestart && currentSongId.value === song_id && isPlaying.value) {
    return
  }

  // 如果是同首歌且暂停中，但不是自动播放触发的重播（即用户手动点播放）
  // 且不是历史歌曲，则执行恢复
  if (!forceRestart && currentSongId.value === song_id && !isPlaying.value && !isHistorySong.value) {
    await window.api.resume()
    isPlaying.value = true
    return
  }

  // 开始切歌/重播流程
  isSwitching.value = true
  playToken++
  const token = playToken

  try {
    // 1. 获取详情和 URL
    const song = await getSongDetail(song_id)
    const url = await getSongUrl(song_id,  configStore.soundQuality)

    if (!song || !url) {
      isSwitching.value = false
      return
    }

    // 2. 停止旧播放（重要：确保底层状态重置）
    try { await window.api.pause() } catch {}

    // 3. 调用底层播放
    // 注意：如果是单曲循环，startTime 为 0
    await window.api.play_url(url, startTime / 1000)

    // 4. 更新 UI 状态
    setPlayerData(song, true)
    isHistorySong.value = false
    currentTime.value = startTime

    // 5. 维护播放列表
    const exists = playlist.value.some(s => s.id === song_id)
    if (!exists) {
      playlist.value.splice(currentIndex.value + 1, 0, {
        id: song.id,
        name: song.name,
        artist: song.ar.map((a: any) => a.name).join(', '),
        cover: song.al.picUrl,
        duration: song.dt
      })
    }

    // 6. 重新监听结束事件
    waitForEnd(song_id, token)
  } catch (error) {
    console.error("播放失败:", error)
  } finally {
    isSwitching.value = false
  }
}

  // 3. 播放列表管理
  const setPlaylist = (list: CurrentSong[]) => {
    playlist.value = list
  }

  const clearPlaylist = () => {
    playlist.value = []
  }

  const addToPlaylist = (song: CurrentSong) => {
    if (!playlist.value.some(s => s.id === song.id)) {
      playlist.value.push(song)
    }
  }

  const togglePlayMode = () => {
    const modes: PlayMode[] = ['loop', 'random', 'single']
    const nextIdx = (modes.indexOf(playMode.value) + 1) % modes.length
    playMode.value = modes[nextIdx]
  }

  // --- 原有逻辑保持 ---
  const initFromStorage = async () => {
    if (!currentSongId.value) return
    const song = await getSongDetail(currentSongId.value)
    if (song) {
      setPlayerData(song, false)
      isHistorySong.value = true
    }
  }

  const setPlayerData = (song: Song, playing: boolean = true) => {
    currentSong.value = {
      id: song.id,
      name: song.name,
      artist: song.ar.map((a: any) => a.name).join(', '),
      cover: song.al.picUrl,
      duration: song.dt
    }
    currentSongId.value = song.id
    isPlaying.value = playing
  }

  const togglePlay = async () => {
    if (isPlaying.value) {
      await window.api.pause()
      isPlaying.value = false
      return
    }
    if (isHistorySong.value && currentSongId.value) {
      await playMusic(currentSongId.value, currentTime.value)
      return
    }
    await window.api.resume()
    isPlaying.value = true
  }

  const seek = async (timeInMs: number) => {
    currentTime.value = timeInMs
    await window.api.seek(timeInMs / 1000)
  }

  const toggleFullScreen = () => {
    isFullScreen.value = !isFullScreen.value
  }

  // --- 监听器 (Watchers) ---
  watch(currentSongId, (id) => {
    if (id !== null) localStorage.setItem('currentSongId', id.toString())
    else localStorage.removeItem('currentSongId')
  })

  watch(currentTime, (time) => {
    localStorage.setItem('currentTime', Math.floor(time).toString())
  })

  watch(isPlaying, (val) => {
    if (val) startTimer()
    else stopTimer()
  }, { immediate: true })

  // 持久化播放列表和模式
  watch(playlist, (newList) => {
    localStorage.setItem('playlist', JSON.stringify(newList))
  }, { deep: true })

  watch(playMode, (newMode) => {
    localStorage.setItem('playMode', newMode)
  })

  return {
    currentSong,
    currentSongId,
    currentTime,
    isPlaying,
    isFullScreen,
    isSeeking,
    duration,
    progressPercent,
    playlist,
    playMode,
    currentIndex,
    initFromStorage,
    playAll,
    playMusic,
    togglePlay,
    seek,
    toggleFullScreen,
    playNext,
    playPrev,
    setPlaylist,
    clearPlaylist,
    addToPlaylist,
    togglePlayMode
  }
})
