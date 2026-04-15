import { defineStore } from 'pinia'
import { ref, watch, computed } from 'vue'
import { CurrentSong, Privilege } from '@renderer/types/player'
import { Song } from '@renderer/types/songDetail'
import { SongUrl, SoundQualityType } from '@renderer/types/song'
import { useUserStore } from '../userStore'
import { useConfigStore } from '../configStore'
import { usePlaylistStore } from './playlist'
import { prepareCachedSongSource } from '@renderer/utils/cache'
import { createCurrentSongArtists } from './utils'
import { getAvailableQualities, computePlayableLevel } from './quality'

export const usePlaybackStore = defineStore('playback', () => {
  const currentSong = ref<CurrentSong | null>(
    JSON.parse(localStorage.getItem('currentSong') || 'null')
  )
  const currentSongId = ref<number | null>(Number(localStorage.getItem('currentSongId')) || null)
  const currentTime = ref(Number(localStorage.getItem('currentTime') || 0))
  const isPlaying = ref(false)
  const isHistorySong = ref(true)
  const isSeeking = ref(false)
  const isSwitching = ref(false)

  const userStore = useUserStore()
  const configStore = useConfigStore()
  const playlistStore = usePlaylistStore()

  let progressTimer: ReturnType<typeof setInterval> | null = null
  let playToken = 0

  const duration = computed(() => currentSong.value?.duration || 0)
  const progressPercent = computed(() => {
    if (duration.value <= 0) return 0
    return ((currentTime.value % duration.value) / duration.value) * 100
  })

  // --- 内部逻辑 (辅助函数) ---

  /**
   * 定时同步播放进度
   * 从底层获取当前播放毫秒数并更新 currentTime
   */
  const syncProgress = async (): Promise<void> => {
    if (isSeeking.value) return // 拖动中不同步，避免进度条跳动
    try {
      const progressMs = await window.api.get_progress()
      if (progressMs !== undefined && progressMs !== null) {
        currentTime.value = progressMs
      }
    } catch (error) {
      console.error('同步进度失败:', error)
    }
  }

  /**
   * 启动进度同步计时器
   */
  const startTimer = (): void => {
    if (progressTimer) return
    progressTimer = setInterval(syncProgress, 200) // 每 200ms 同步一次
  }

  /**
   * 停止进度同步计时器
   */
  const stopTimer = (): void => {
    if (progressTimer) {
      clearInterval(progressTimer)
      progressTimer = null
    }
  }

  /**
   * 获取歌曲详情及其权限
   */
  const getSongDetailData = async (
    id: number
  ): Promise<{ song: Song; privilege: Privilege } | undefined> => {
    try {
      const res = (await window.api.song_detail({ ids: [id] })) as {
        body?: { songs: Song[]; privileges: Privilege[] }
      }
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
   * 请求指定音质的播放 URL
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
      console.error('获取歌曲 URL 失败', e)
      return ''
    }
  }

  /**
   * 设置当前播放歌曲的数据并同步状态
   */
  const setPlayerData = (song: Song, playing: boolean = true): void => {
    currentSong.value = {
      id: song.id,
      name: song.name,
      artists: createCurrentSongArtists(song.ar),
      cover: song.al.picUrl,
      duration: song.dt
    }
    currentSongId.value = song.id
    playlistStore.currentSongId = song.id
    isPlaying.value = playing
  }

  /**
   * 异步等待当前音频播放结束
   * @param songId 触发等待时的歌曲 ID
   * @param token 触发等待时的播放请求 token
   */
  const waitForEnd = async (songId: number, token: number): Promise<void> => {
    try {
      // 阻塞式等待底层音频播放结束信号
      await window.api.wait_finished()

      // 验证环境：如果 token 或歌曲 ID 已变，说明此监听已过期，不触发自动切换
      if (token !== playToken) return
      if (isSwitching.value) return
      if (currentSongId.value !== songId) return

      // 播放结束后的状态清理
      isPlaying.value = false
      isHistorySong.value = true
      stopTimer()
      currentTime.value = duration.value

      // 自动播放下一首
      await playNext(true)
    } catch {
      // 播放中断或 API 调用失败，通常不处理
    }
  }

  // --- 公开操作 (Actions) ---

  /**
   * 核心播放入口：播放单首歌曲
   * @param song_id 歌曲 ID
   * @param startTime 起始播放时间 (ms)
   * @param forceRestart 是否强制重新加载播放 (即使是同一首歌)
   */
  const playMusic = async (
    song_id: number,
    startTime: number = 0,
    forceRestart: boolean = false
  ): Promise<void> => {
    // 1. 状态检查：如果是当前正在播放的同一首歌，且没有强制重启，则直接返回
    if (!forceRestart && currentSongId.value === song_id && isPlaying.value) {
      return
    }

    // 2. 准备开始切换流程
    // 允许新任务中断正在进行的旧任务。我们通过 playToken 来标识每个任务的唯一性。
    playToken++
    const currentToken = playToken
    isSwitching.value = true

    try {
      // 3. 继续播放逻辑：如果是同一首歌但处于暂停状态，且非历史回放，直接恢复播放
      if (
        !forceRestart &&
        currentSongId.value === song_id &&
        !isPlaying.value &&
        !isHistorySong.value
      ) {
        await configStore.ensureConfiguredOutputDevice()
        await window.api.resume()
        if (currentToken === playToken) {
          isPlaying.value = true
        }
        return
      }

      // 4. 获取歌曲详情：包含歌曲信息和播放权限
      const detailData = await getSongDetailData(song_id)
      // 在每一个异步步长后，都要检查任务是否已过期
      if (currentToken !== playToken) return

      if (!detailData) {
        throw new Error('无法获取歌曲详情')
      }

      const { song, privilege } = detailData

      // 5. 确定最佳播放音质：结合可用音质和用户设置
      const availableQualities = getAvailableQualities(song, privilege)
      const targetLevel = computePlayableLevel(availableQualities, configStore.soundQuality)

      // 6. 获取歌曲播放地址 (URL)
      const url = await fetchSongUrl(song_id, targetLevel)
      if (currentToken !== playToken) return
      if (!url) {
        throw new Error('获取播放 URL 失败')
      }

      // 7. 停止当前正在播放的所有音频
      try {
        await window.api.stop()
      } catch {
        // 忽略停止时的报错
      }
      if (currentToken !== playToken) return

      // 8. 确保输出设备已就绪
      await configStore.ensureConfiguredOutputDevice()
      if (currentToken !== playToken) return

      // 9. 准备播放源：处理缓存逻辑 (可能是本地文件，也可能是带缓存的 URL)
      const playbackSource = await prepareCachedSongSource(song_id, targetLevel, url)
      if (currentToken !== playToken) return

      // 10. 执行底层播放指令
      const startTimeInSeconds = startTime / 1000
      if (playbackSource.type === 'file') {
        // 情况 A: 命中本地文件缓存，直接播放文件
        await window.api.play_file(playbackSource.value, startTimeInSeconds)
      } else if (playbackSource.cachePath && playbackSource.metadataPath) {
        // 情况 B: 支持边播边存，调用带缓存功能的播放接口
        await window.api.play_url_cached(
          playbackSource.value,
          playbackSource.cachePath,
          playbackSource.metadataPath,
          song.dt,
          playbackSource.cacheAheadSecs ?? configStore.songCacheAheadSecs,
          startTimeInSeconds
        )
      } else {
        // 情况 C: 普通网络 URL 播放
        await window.api.play_url(playbackSource.value, startTimeInSeconds)
      }

      // 再次检查 token，确保在调用播放 API 期间没有新请求进入
      if (currentToken !== playToken) return

      // 11. 更新播放器状态
      setPlayerData(song, true)
      isHistorySong.value = false
      currentTime.value = startTime

      // 12. 自动将该歌曲加入播放列表 (如果不在列表中)
      const exists = playlistStore.playlist.some((s) => s.id === song_id)
      if (!exists) {
        playlistStore.addToPlaylist({
          id: song.id,
          name: song.name,
          artists: createCurrentSongArtists(song.ar),
          cover: song.al.picUrl,
          duration: song.dt
        })
      }

      // 13. 注册监听：当歌曲自然结束时自动播放下一首
      waitForEnd(song_id, currentToken)
    } catch (error) {
      // 只有当前任务未过期时才更新错误状态
      if (currentToken === playToken) {
        console.error('播放全流程失败:', error)
        isPlaying.value = false
      }
    } finally {
      // 14. 仅当当前任务是最新任务时，才重置切换标记
      if (currentToken === playToken) {
        isSwitching.value = false
      }
    }
  }

  /**
   * 播放全部 (重置播放列表)
   */
  const playAll = async (list: CurrentSong[], startIndex = 0): Promise<void> => {
    if (list.length === 0) return
    playlistStore.setPlaylist(list)
    const targetSong = list[startIndex]
    await playMusic(targetSong.id)
  }

  /**
   * 播放/暂停 切换
   */
  const togglePlay = async (): Promise<void> => {
    // 正在切换中不响应
    if (isSwitching.value) return

    // 如果当前正在播放，则暂停
    if (isPlaying.value) {
      await window.api.pause()
      isPlaying.value = false
      return
    }

    // 如果当前处于历史回放状态 (未真正加载到播放器中)，重新播放该曲
    if (isHistorySong.value && currentSongId.value) {
      await playMusic(currentSongId.value, currentTime.value)
      return
    }

    // 普通恢复播放
    await configStore.ensureConfiguredOutputDevice()
    await window.api.resume()
    isPlaying.value = true
  }

  /**
   * 跳转播放进度
   * @param timeInMs 目标时间 (毫秒)
   */
  const seek = async (timeInMs: number): Promise<void> => {
    currentTime.value = timeInMs
    await window.api.seek(timeInMs / 1000)
  }

  /**
   * 播放下一首
   * @param isAuto 是否为自然播放结束触发
   */
  const playNext = async (isAuto = false): Promise<void> => {
    const nextId = playlistStore.getNextSongId(isAuto)
    if (nextId !== null) {
      await playMusic(nextId)
    }
  }

  /**
   * 播放上一首
   */
  const playPrev = async (): Promise<void> => {
    const prevId = playlistStore.getPrevSongId()
    if (prevId !== null) {
      await playMusic(prevId)
    }
  }

  /**
   * 停止播放并清空状态
   */
  const stop = async (): Promise<void> => {
    try {
      await window.api.stop()
    } catch {
      // ignore
    }
    currentSong.value = null
    currentSongId.value = null
    isPlaying.value = false
    currentTime.value = 0
    isHistorySong.value = true
    stopTimer()
  }

  /**
   * 从本地缓存初始化播放器状态 (持久化恢复)
   */
  const initFromStorage = async (): Promise<void> => {
    if (!currentSongId.value) return

    // 如果缓存中有歌曲数据，先显示缓存数据，避免界面空白
    if (currentSong.value && currentSong.value.id === currentSongId.value) {
      isHistorySong.value = true
      // 仍然尝试异步更新一下最新详情 (后台执行，不阻塞 UI)
      getSongDetailData(currentSongId.value).then((data) => {
        if (data && data.song) {
          setPlayerData(data.song, false)
          isHistorySong.value = true
        }
      })
      return
    }

    // 彻底没有缓存数据，则同步加载
    const data = await getSongDetailData(currentSongId.value)
    if (data && data.song) {
      setPlayerData(data.song, false)
      isHistorySong.value = true
    }
  }

  // --- 监听器 ---

  watch(currentSong, (song) => {
    if (song) {
      localStorage.setItem('currentSong', JSON.stringify(song))
    } else {
      localStorage.removeItem('currentSong')
    }
  })

  watch(currentSongId, (id) => {
    if (id !== null) {
      localStorage.setItem('currentSongId', id.toString())
      playlistStore.currentSongId = id
    } else {
      localStorage.removeItem('currentSongId')
      playlistStore.currentSongId = null
    }
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

  return {
    currentSong,
    currentSongId,
    currentTime,
    isPlaying,
    isSeeking,
    duration,
    progressPercent,
    playMusic,
    togglePlay,
    seek,
    playNext,
    playPrev,
    stop,
    initFromStorage,
    playAll
  }
})
