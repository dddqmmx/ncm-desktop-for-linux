import { Song, SongDetailResult } from '@renderer/types/songDetail'
import { defineStore } from 'pinia'
import { ref, watch, computed } from 'vue'
import { useUserStore } from './userStore'
import { SoundQualityType } from 'NeteaseCloudMusicApi'
import { SongUrl } from '@renderer/types/song'

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
  const isHistorySong = ref(true) // 标记是否为历史记录中的歌曲（未真正开始播放）

  const userStore = useUserStore()
  let progressTimer: ReturnType<typeof setInterval> | null = null

  // --- 计算属性 (Getters) ---
  const duration = computed(() => currentSong.value?.duration || 0)
  const progressPercent = computed(() => {
    if (duration.value <= 0) return 0
    // 使用取余操作，确保 currentTime 超过 duration 时（如循环播放），进度条能正确回到起点
    return ((currentTime.value % duration.value) / duration.value) * 100
  })

  // --- 私有辅助函数 ---
  const getSongDetail = async (id: number): Promise<Song | undefined> => {
    const res = await window.api.song_detail({ ids: [id] }) as { body?: SongDetailResult }
    return res.body?.songs?.[0]
  }

  const getSongUrl = async (song_id: number): Promise<string> => {
    const res = await window.api.song_url({
      id: song_id,
      level: "standard" as SoundQualityType,
      cookie: userStore.cookie
    }) as { body?: { data?: SongUrl[] } }
    return res.body?.data?.[0].url ?? ""
  }

  // 同步后端进度到 Store
const syncProgress = async () => {
  try {
    const progressMs = await window.api.get_progress();
    console.log('收到原始进度:', progressMs); // <-- 添加这一行
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
  const waitForEnd = async (songId: number) => {
    try {
      await window.api.wait_finished()

      // ❗如果已经切歌，直接忽略
      if (currentSongId.value !== songId) return

      isPlaying.value = false
      stopTimer()
      currentTime.value = duration.value

      // 👉 自动下一首 / 单曲循环 放这里
    } catch {
      // ignore
    }
  }

  // 初始化：从本地存储恢复歌曲信息
  const initFromStorage = async () => {
    if (!currentSongId.value) return
    const song = await getSongDetail(currentSongId.value)
    if (song) {
      setPlayerData(song, false)
      isHistorySong.value = true // 标记这是历史记录，需要特殊逻辑恢复
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

  // 播放新歌曲
  const playMusic = async (song_id: number, startTime: number = 0) => {
    // 设置当前时间（如果是新歌则为0，如果是恢复历史则为旧进度）
    currentTime.value = startTime

    const song = await getSongDetail(song_id)
    if (!song) return

    const url = await getSongUrl(song_id)
    if (!url) return

    console.log(url)

    // 更新播放器状态
    setPlayerData(song, true) // 内部通常会设置 isPlaying.value = true
    isHistorySong.value = false

    // 调用 API 播放，并传入起始时间（秒）
    await window.api.play_url(url, startTime / 1000)

    // 监听结束
    waitForEnd(song_id)
  }

  const togglePlay = async () => {
    // 1. 如果正在播放 -> 暂停
    if (isPlaying.value) {
      await window.api.pause()
      isPlaying.value = false
      return
    }

    // 2. 如果是历史记录中的歌曲（例如刚打开 App 或切换回来）
    if (isHistorySong.value && currentSongId.value) {
      // 调用 playMusic，传入记录的当前时间
      await playMusic(currentSongId.value, currentTime.value)
      return
    }

    // 3. 普通的从暂停中恢复
    await window.api.resume()
    isPlaying.value = true
  }

  // 跳转进度
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

  return {
    currentSong,
    currentSongId,
    currentTime,
    isPlaying,
    isFullScreen,
    duration,
    progressPercent,
    initFromStorage,
    playMusic,
    togglePlay,
    seek,
    toggleFullScreen
  }
})
