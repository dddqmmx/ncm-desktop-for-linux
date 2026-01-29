import { Song, SongDetailResult } from '@renderer/types/songDetail'
import { defineStore } from 'pinia'
import { ref, watch, computed } from 'vue'
import { useUserStore } from './userStore'
import { SoundQualityType } from 'NeteaseCloudMusicApi'
import { SongUrl } from '@renderer/types/song'

// 播放模式定义
export type PlayMode = 'sequence' | 'loop' | 'random' | 'single'

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

  // --- 播放列表相关状态 ---
  const playlist = ref<CurrentSong[]>(JSON.parse(localStorage.getItem('playlist') || '[]'))
  const playMode = ref<PlayMode>((localStorage.getItem('playMode') as PlayMode) || 'sequence')

  const userStore = useUserStore()
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

  // --- 私有辅助函数 ---
  const getSongDetail = async (id: number): Promise<Song | undefined> => {
    const res = await window.api.song_detail({ ids: [id] }) as { body?: SongDetailResult }
    return res.body?.songs?.[0]
  }

  const getSongUrl = async (song_id: number): Promise<string> => {
    const res = await window.api.song_url({
      id: song_id,
      level: "hires" as SoundQualityType,
      cookie: userStore.cookie
    }) as { body?: { data?: SongUrl[] } }
    return res.body?.data?.[0].url ?? ""
  }

  const syncProgress = async () => {
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

  // 1. 播放下一首 (isAuto: 是否为播放结束自动触发)
  const playNext = async (isAuto = false) => {
    if (playlist.value.length === 0) return

    // 单曲循环逻辑
    if (isAuto && playMode.value === 'single') {
      await playMusic(currentSongId.value!, 0)
      return
    }

    let nextIndex = 0
    if (playMode.value === 'random') {
      nextIndex = Math.floor(Math.random() * playlist.value.length)
    } else {
      nextIndex = currentIndex.value + 1
      if (nextIndex >= playlist.value.length) {
        nextIndex = 0 // 列表循环
      }
    }

    const nextSong = playlist.value[nextIndex]
    await playMusic(nextSong.id)
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

  const waitForEnd = async (songId: number) => {
    try {
      await window.api.wait_finished()
      if (currentSongId.value !== songId) return

      isPlaying.value = false
      stopTimer()
      currentTime.value = duration.value

      // 关键：播放结束后自动根据模式播放下一首
      await playNext(true)
    } catch {
      // ignore
    }
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

  // 修改 playMusic，避免在 playAll 时产生冗余逻辑
  const playMusic = async (song_id: number, startTime: number = 0) => {
    currentTime.value = startTime
    const song = await getSongDetail(song_id)
    if (!song) return

    const url = await getSongUrl(song_id)
    if (!url) return

    setPlayerData(song, true)
    isHistorySong.value = false

    // 关键优化：如果当前列表里没有这首歌，则插入到下一首
    // 如果是通过 playAll 进来的，playlist 已经包含了这首歌，这里不会重复插入
    const exists = playlist.value.some(s => s.id === song_id)
    if (!exists) {
      const newSong: CurrentSong = {
        id: song.id,
        name: song.name,
        artist: song.ar.map((a: any) => a.name).join(', '),
        cover: song.al.picUrl,
        duration: song.dt
      }
      playlist.value.splice(currentIndex.value + 1, 0, newSong)
    }

    await window.api.play_url(url, startTime / 1000)
    waitForEnd(song_id)
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
    const modes: PlayMode[] = ['sequence', 'loop', 'random', 'single']
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
