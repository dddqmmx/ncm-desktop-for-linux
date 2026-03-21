import { Song } from '@renderer/types/songDetail'
import { defineStore } from 'pinia'
import { ref, watch, computed } from 'vue'
import { useUserStore } from './userStore'
import { SongUrl, type SoundQualityType } from '@renderer/types/song'
import { useConfigStore } from './configStore'

// 播放模式定义
export type PlayMode = 'loop' | 'random' | 'single'

export interface CurrentSong {
  id: number
  name: string
  artist: string
  cover: string
  duration: number
}

// 新增：权限对象定义（用于判断新音质）
interface Privilege {
  id: number
  playMaxBrLevel: string // 关键字段：jymaster, lossless, exhigh 等
  chargeInfoList: unknown[]
}

// 扩展 API 返回类型
interface ExtendedSongDetailResult {
  songs: Song[]
  privileges: Privilege[]
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

  const currentIndex = computed(() => {
    return playlist.value.findIndex((s) => s.id === currentSongId.value)
  })

  // --- 音质配置 ---

  // 降级顺序（从高到低）
  const downgradeOrder: SoundQualityType[] = [
    'jymaster', // 超清母带
    'sky',      // 沉浸声
    'jyeffect', // 高清杜比
    'hires',    // Hi-Res
    'lossless', // 无损
    'exhigh',   // 极高 (320k)
    'standard'  // 标准 (128k)
  ]

  // --- 私有辅助函数 ---

  /**
   * 获取歌曲详情和权限信息
   * @returns 返回包含 song 和 privilege 的对象，如果失败返回 undefined
   */
  const getSongDetailData = async (id: number): Promise<{ song: Song; privilege: Privilege } | undefined> => {
    try {
      const res = (await window.api.song_detail({ ids: [id] })) as {
        body?: ExtendedSongDetailResult
      }
      // 必须同时存在 songs 和 privileges 才算成功
      if (res.body?.songs?.[0] && res.body?.privileges?.[0]) {
        return {
          song: res.body.songs[0],
          privilege: res.body.privileges[0]
        }
      }
    } catch (e) {
      console.error('获取歌曲详情失败', e)
    }
    return undefined
  }

  /**
   * 获取当前歌曲支持的所有音质列表
   */
  const getAvailableQualities = (song: Song, privilege: Privilege): SoundQualityType[] => {
    const available: SoundQualityType[] = ['standard'] // 默认支持标准

    // 1. 检查旧版字段 (存在即代表有资源)
    if (song.h) available.push('exhigh')
    if (song.sq) available.push('lossless')
    if (song.hr) available.push('hires')

    // 2. 检查新版权限字段 (Privilege)
    const level = privilege.playMaxBrLevel

    if (level === 'jymaster') {
      available.push('jymaster')
      // 通常支持 master 也意味着支持以下音质，补全以防万一
      if (!available.includes('lossless')) available.push('lossless')
      if (!available.includes('exhigh')) available.push('exhigh')
    }

    if (level === 'sky') available.push('sky')
    if (level === 'jyeffect') available.push('jyeffect')

    // 去重
    return Array.from(new Set(available))
  }

  /**
   * 根据目标音质计算最终请求的音质 (Level)
   */
  const computePlayableLevel = (
    available: SoundQualityType[],
    targetQuality: SoundQualityType
  ): SoundQualityType => {
    // 如果目标音质直接可用
    if (available.includes(targetQuality)) {
      return targetQuality
    }

    // 否则按降级顺序查找
    const targetIndex = downgradeOrder.indexOf(targetQuality)
    // 从目标音质的下一级开始找
    for (let i = targetIndex + 1; i < downgradeOrder.length; i++) {
      const quality = downgradeOrder[i]
      if (available.includes(quality)) {
        console.log(`[音质降级] 目标: ${targetQuality} -> 实际: ${quality}`)
        return quality
      }
    }

    return 'standard' // 兜底
  }

  /**
   * 纯粹的 URL 获取函数
   */
  const fetchSongUrl = async (song_id: number, level: SoundQualityType): Promise<string> => {
    try {
      const res = (await window.api.song_url({
        id: song_id,
        level: level,
        cookie: userStore.cookie
      })) as { body?: { data?: SongUrl[] } }
      return res.body?.data?.[0].url ?? ''
    } catch (e) {
      console.error('获取歌曲URL失败', e)
      return ''
    }
  }

  const syncProgress = async (): Promise<void> => {
    if (isSeeking.value) return
    try {
      const progressMs = await window.api.get_progress()
      if (progressMs !== undefined && progressMs !== null) {
        currentTime.value = progressMs
      }
    } catch (error) {
      console.error('同步进度失败:', error)
    }
  }

  const prepareOutputDeviceForPlayback = async (): Promise<void> => {
    const configuredDeviceId = configStore.outputDeviceId
    const appliedDeviceId = await configStore.ensureConfiguredOutputDevice()

    if (
      configuredDeviceId &&
      configuredDeviceId !== 'default' &&
      appliedDeviceId === 'default'
    ) {
      console.warn(
        `[音频设备] 已配置设备 ${configuredDeviceId} 不可用，播放前已自动回退到系统默认输出。`
      )
    }
  }

  const startTimer = (): void => {
    if (progressTimer) return
    progressTimer = setInterval(syncProgress, 100)
  }

  const stopTimer = (): void => {
    if (progressTimer) {
      clearInterval(progressTimer)
      progressTimer = null
    }
  }

  // --- 核心操作 (Actions) ---

  const playNext = async (isAuto = false): Promise<void> => {
    if (playlist.value.length === 0) return

    // 1. 处理单曲循环
    if (isAuto && playMode.value === 'single' && currentSongId.value) {
      await playMusic(currentSongId.value, 0, true)
      return
    }

    // 2. 计算下一首的索引
    let nextIndex = 0
    if (playMode.value === 'random') {
      nextIndex = Math.floor(Math.random() * playlist.value.length)
    } else {
      nextIndex = currentIndex.value + 1
      if (nextIndex >= playlist.value.length) {
        nextIndex = 0
      }
    }

    const nextSong = playlist.value[nextIndex]
    if (nextSong) {
      await playMusic(nextSong.id)
    }
  }

  const playPrev = async (): Promise<void> => {
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

  let playToken = 0
  const isSwitching = ref(false)

  const waitForEnd = async (songId: number, token: number): Promise<void> => {
    try {
      await window.api.wait_finished()
      if (token !== playToken) return
      if (isSwitching.value) return
      if (currentSongId.value !== songId) return

      isPlaying.value = false
      stopTimer()
      currentTime.value = duration.value
      await playNext(true)
    } catch {
      // ignore
    }
  }

  const playAll = async (list: CurrentSong[], startIndex = 0): Promise<void> => {
    if (list.length === 0) return
    playlist.value = [...list]
    const targetSong = list[startIndex]
    await playMusic(targetSong.id)
  }

  /**
   * 核心播放逻辑 (已修正音质选择)
   */
  const playMusic = async (
    song_id: number,
    startTime: number = 0,
    forceRestart: boolean = false
  ): Promise<void> => {
    if (isSwitching.value) return

    if (!forceRestart && currentSongId.value === song_id && isPlaying.value) {
      return
    }

    isSwitching.value = true
    playToken++
    const token = playToken

    try {
      if (
        !forceRestart &&
        currentSongId.value === song_id &&
        !isPlaying.value &&
        !isHistorySong.value
      ) {
        await prepareOutputDeviceForPlayback()
        await window.api.resume()
        isPlaying.value = true
        return
      }

      // 1. 获取完整详情 (含权限)
      const detailData = await getSongDetailData(song_id)

      if (!detailData) {
        console.error('无法获取歌曲详情')
        isSwitching.value = false
        return
      }

      const { song, privilege } = detailData

      // 2. 智能计算音质
      const availableQualities = getAvailableQualities(song, privilege)
      const targetLevel = computePlayableLevel(availableQualities, configStore.soundQuality)

      console.log(`[播放信息] ID:${song_id} | 目标音质:${configStore.soundQuality} | 实际请求:${targetLevel}`)

      // 3. 获取 URL
      const url = await fetchSongUrl(song_id, targetLevel)

      if (!url) {
        console.error('无法获取播放链接')
        // 这里可以添加自动切歌逻辑或者错误提示
        isSwitching.value = false
        return
      }

      try {
        await window.api.pause()
      } catch {
        // ignore
      }

      await prepareOutputDeviceForPlayback()
      await window.api.play_url(url, startTime / 1000)

      setPlayerData(song, true)
      isHistorySong.value = false
      currentTime.value = startTime

      const exists = playlist.value.some((s) => s.id === song_id)
      if (!exists) {
        playlist.value.splice(currentIndex.value + 1, 0, {
          id: song.id,
          name: song.name,
          artist: song.ar.map((artist) => artist.name).join(', '),
          cover: song.al.picUrl,
          duration: song.dt
        })
      }

      waitForEnd(song_id, token)
    } catch (error) {
      console.error('播放失败:', error)
    } finally {
      isSwitching.value = false
    }
  }

  const setPlaylist = (list: CurrentSong[]): void => {
    playlist.value = list
  }

  const clearPlaylist = (): void => {
    playlist.value = []
  }

  const addToPlaylist = (song: CurrentSong): void => {
    if (!playlist.value.some((s) => s.id === song.id)) {
      playlist.value.push(song)
    }
  }

  const togglePlayMode = (): void => {
    const modes: PlayMode[] = ['loop', 'random', 'single']
    const nextIdx = (modes.indexOf(playMode.value) + 1) % modes.length
    playMode.value = modes[nextIdx]
  }

  // --- 初始化逻辑 ---
  const initFromStorage = async (): Promise<void> => {
    if (!currentSongId.value) return
    // 初始化时也需要获取新的 song 结构来更新 UI，虽然这里为了简单只取 song
    // 注意：initFromStorage 只是为了 UI 显示，不需要 privilege
    const data = await getSongDetailData(currentSongId.value)
    if (data && data.song) {
      setPlayerData(data.song, false)
      isHistorySong.value = true
    }
  }

  const setPlayerData = (song: Song, playing: boolean = true): void => {
    currentSong.value = {
      id: song.id,
      name: song.name,
      artist: song.ar.map((artist) => artist.name).join(', '),
      cover: song.al.picUrl,
      duration: song.dt
    }
    currentSongId.value = song.id
    isPlaying.value = playing
  }

  const togglePlay = async (): Promise<void> => {
    if (isPlaying.value) {
      await window.api.pause()
      isPlaying.value = false
      return
    }
    if (isHistorySong.value && currentSongId.value) {
      await playMusic(currentSongId.value, currentTime.value)
      return
    }
    await prepareOutputDeviceForPlayback()
    await window.api.resume()
    isPlaying.value = true
  }

  const seek = async (timeInMs: number): Promise<void> => {
    currentTime.value = timeInMs
    await window.api.seek(timeInMs / 1000)
  }

  const toggleFullScreen = (): void => {
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

  watch(
    isPlaying,
    (val) => {
      if (val) startTimer()
      else stopTimer()
    },
    { immediate: true }
  )

  watch(
    playlist,
    (newList) => {
      localStorage.setItem('playlist', JSON.stringify(newList))
    },
    { deep: true }
  )

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
