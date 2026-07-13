import { defineStore } from 'pinia'
import { ref, watch, computed } from 'vue'
import { CurrentSong } from '@renderer/types/player'
import { Song } from '@renderer/types/songDetail'
import { SongUrl, SoundQualityType } from '@renderer/types/song'
import type {
  PlaybackCacheEngine,
  PreparedPlaybackCache,
  SongCacheProgress
} from '@renderer/types/cache'
import { useUserStore } from '../userStore'
import { useConfigStore } from '../configStore'
import { useDialogStore } from '../dialogStore'
import { usePlaylistStore } from './playlist'
import { preparePlaybackCache, startPlaybackBackgroundCache } from '@renderer/utils/cache'
import { createCurrentSongArtists } from './utils'
import { isSoundQualityLevel, getFallbackQualities } from './quality'
import { webAudioEngine } from './webAudioEngine'

export const usePlaybackStore = defineStore('playback', () => {
  const currentSong = ref<CurrentSong | null>(
    JSON.parse(localStorage.getItem('currentSong') || 'null')
  )
  const currentSongId = ref<number | null>(Number(localStorage.getItem('currentSongId')) || null)
  const currentTime = ref(Number(localStorage.getItem('currentTime') || 0))
  const isPlaying = ref(false)
  const isHistorySong = ref(true)
  const isSeeking = ref(false)
  const isBuffering = ref(false)
  const isSwitching = ref(false)
  const isLoading = ref(false)
  // [调试] 后端上报的原始播放位置（未经媒体时钟处理），用于定位延迟到底卡在哪一环
  const rawProgressMs = ref(0)
  const playbackError = ref('')
  const bufferedPercent = ref(0)

  const userStore = useUserStore()
  const configStore = useConfigStore()
  const dialogStore = useDialogStore()
  const playlistStore = usePlaylistStore()

  let progressTimer: ReturnType<typeof setInterval> | null = null
  let progressAnimationFrame: number | null = null
  let lastSyncedProgressMs = currentTime.value
  let lastSyncedAt = performance.now()
  let lastPersistedCurrentTime = Math.floor(currentTime.value)
  let playToken = 0
  let activeCacheMetadataPath = ''
  let lastCachePlaybackPositionMs = -1
  let loadingStartedAt = 0
  let loadingExpectedStartTime = 0

  const PLAYBACK_OPERATION_TIMEOUT_MS = 20_000
  const STARTUP_STALL_TIMEOUT_MS = 30_000
  const NATURAL_END_TOLERANCE_MS = 2_500
  const PLAYBACK_STARTED_PROGRESS_THRESHOLD_MS = 150
  const CACHE_PROGRESS_POLL_INTERVAL_MS = 100
  const NATIVE_PROGRESS_POLL_INTERVAL_MS = 200
  // ====== 严格同步配置 ======
  // 同步时间平滑阈值：超过此阈值的误差将直接触发跳变，而不是平滑过渡。
  // 由于现在底层是 16ms 刷新率 + RTT 延迟补偿，因此阈值可以缩减到极小（20ms，一帧多一点）
  const DRIFT_THRESHOLD_MS = 20

  const duration = computed(() => currentSong.value?.duration || 0)
  const progressPercent = computed(() => {
    if (duration.value <= 0) return 0
    const clampedTime = Math.min(Math.max(currentTime.value, 0), duration.value)
    return (clampedTime / duration.value) * 100
  })

  // --- 内部逻辑 (辅助函数) ---

  const withTimeout = async <T>(
    promise: Promise<T>,
    timeoutMs: number,
    message: string
  ): Promise<T> => {
    let timeout: ReturnType<typeof setTimeout> | null = null

    try {
      return await Promise.race([
        promise,
        new Promise<T>((_, reject) => {
          timeout = setTimeout(() => reject(new Error(message)), timeoutMs)
        })
      ])
    } finally {
      if (timeout) {
        clearTimeout(timeout)
      }
    }
  }

  const resetPlaybackLoadState = (): void => {
    console.log(`[SYNC] resetPlaybackLoadState: isLoading false (was ${isLoading.value})`)
    isLoading.value = false
    loadingStartedAt = 0
    loadingExpectedStartTime = 0
  }

  const beginPlaybackLoadState = (expectedStartTime = 0): void => {
    console.log(
      `[SYNC] beginPlaybackLoadState: isLoading=true, expectedStart=${expectedStartTime}ms`
    )
    playbackError.value = ''
    isLoading.value = true
    loadingStartedAt = Date.now()
    loadingExpectedStartTime = expectedStartTime
  }

  const updateCacheProgress = async (): Promise<void> => {
    if (!activeCacheMetadataPath) return

    try {
      const progress = (await window.api.get_cached_song_progress(
        activeCacheMetadataPath
      )) as SongCacheProgress
      if (progress && typeof progress.percent === 'number' && Number.isFinite(progress.percent)) {
        bufferedPercent.value = Math.max(bufferedPercent.value, Math.min(100, progress.percent))
      }
    } catch (error) {
      console.warn('读取歌曲预缓存进度失败:', error)
    }
  }

  const syncSongCachePlaybackPosition = async (positionMs?: number): Promise<void> => {
    if (configStore.audioEngine !== 'webapi' || !activeCacheMetadataPath) return
    const normalizedPositionMs = Math.max(0, Math.round(positionMs ?? webAudioEngine.currentTimeMs))
    if (normalizedPositionMs === lastCachePlaybackPositionMs) return

    lastCachePlaybackPositionMs = normalizedPositionMs
    try {
      await window.api.update_song_cache_playback_position({
        metadataPath: activeCacheMetadataPath,
        playbackPositionMs: normalizedPositionMs
      })
    } catch (error) {
      console.warn('更新歌曲缓存播放进度失败:', error)
    }
  }

  const cancelActiveSongCacheDownload = async (): Promise<void> => {
    const metadataPath = activeCacheMetadataPath
    activeCacheMetadataPath = ''
    lastCachePlaybackPositionMs = -1
    if (!metadataPath) return

    try {
      await window.api.cancel_song_cache_download(metadataPath)
    } catch (error) {
      console.warn('停止歌曲后台缓存失败:', error)
    }
  }

  const stopAfterPlaybackFailure = async (message: string): Promise<void> => {
    playbackError.value = message
    resetPlaybackLoadState()
    isPlaying.value = false
    isHistorySong.value = true
    stopTimer()

    try {
      await window.api.stop()
    } catch {
      // ignore
    }
  }

  /**
   * 定时同步播放进度
   * 从底层获取当前播放毫秒数并更新 currentTime
   * 使用漂移平滑校正：小漂移直接吸收，大漂移立即纠正
   *
   * 关键设计：即使在 seek 拖拽期间也不完全跳过，
   * 因为 isLoading 的清除逻辑在此函数内——如果完全跳过，
   * isLoading 永远无法被清除，导致 seek 后歌词冻结。
   */
  let ignoreDriftUntil = 0

  const syncProgress = async (): Promise<void> => {
    try {
      const reqStart = performance.now()
      const [backendProgressMs, isBufferingNow] = await Promise.all([
        window.api.get_progress(),
        window.api.is_buffering()
      ])
      const reqEnd = performance.now()
      isBuffering.value = isBufferingNow === true

      let progressMs = backendProgressMs

      if (backendProgressMs !== undefined && backendProgressMs !== null) {
        rawProgressMs.value = backendProgressMs
        // --- 核心优化：IPC RTT 延迟补偿 ---
        // get_progress 需要经过 JS -> IPC -> Rust -> IPC -> JS 的漫长旅途。
        // 我们假定往返时间是对称的，所以拿到数据的瞬间，真实音频已经又往前播放了 rtt / 2。
        const rtt = reqEnd - reqStart
        // 补偿后的精准进度（仅在非暂停时，或者说是简单加上偏移量。为了严谨，我们直接加上补偿量）
        progressMs = isPlaying.value ? backendProgressMs + rtt / 2 : backendProgressMs

        const now = performance.now()

        if (isSeeking.value) {
          console.log(
            `[SYNC] syncProgress (SEEKING): raw=${backendProgressMs}ms, rtt=${rtt.toFixed(1)}ms. updating anchors only. isLoading=${isLoading.value}`
          )
          lastSyncedProgressMs = progressMs
          lastSyncedAt = now
        } else {
          const interpolatedEstimate = lastSyncedProgressMs + (now - lastSyncedAt)
          const drift = Math.abs(progressMs - interpolatedEstimate)

          if (drift <= DRIFT_THRESHOLD_MS) {
            lastSyncedProgressMs = progressMs
            lastSyncedAt = now
          } else if (now >= ignoreDriftUntil) {
            console.log(
              `[SYNC] syncProgress: LARGE DRIFT ${drift.toFixed(0)}ms, correcting currentTime ${currentTime.value} -> ${progressMs}`
            )
            currentTime.value = progressMs
            lastSyncedProgressMs = progressMs
            lastSyncedAt = now
          } else {
            console.log(
              `[SYNC] syncProgress: IGNORING LARGE DRIFT ${drift.toFixed(0)}ms (immune after seek), realProgress=${progressMs}ms`
            )
            // 只更新锚点不更新 currentTime，保持现有的 UI 进度，给后端时间追上来
            lastSyncedProgressMs = currentTime.value
            lastSyncedAt = now
          }
        }
      }

      await updateCacheProgress()

      if (isLoading.value) {
        const hasStarted =
          typeof progressMs === 'number' &&
          !isBuffering.value &&
          hasPlaybackReachedExpectedStart(progressMs)
        console.log(
          `[SYNC] syncProgress: isLoading=true, realProgress=${progressMs}ms, expected=${loadingExpectedStartTime}ms, isBuffering=${isBuffering.value}, hasStarted=${hasStarted}`
        )

        if (hasStarted) {
          console.log(`[SYNC] syncProgress: CLEARING isLoading! Audio reached expected position.`)
          resetPlaybackLoadState()
        } else if (loadingStartedAt && Date.now() - loadingStartedAt > STARTUP_STALL_TIMEOUT_MS) {
          await stopAfterPlaybackFailure('网络较慢，音乐加载超时。')
        }
      }
    } catch (error) {
      console.error('同步进度失败:', error)
    }
  }

  const hasPlaybackReachedExpectedStart = (progressMs: number): boolean => {
    if (loadingExpectedStartTime <= 0) {
      return progressMs >= PLAYBACK_STARTED_PROGRESS_THRESHOLD_MS
    }

    return progressMs >= loadingExpectedStartTime
  }

  let _syncLocalLogCounter = 0
  const syncLocalProgress = (rafTimestamp: number): void => {
    if (isWebEngine.value) {
      // WebAPI 引擎：直接读 <audio> 的真实播放位置驱动歌词/进度
      if (!isSeeking.value) {
        const ms = Math.round(webAudioEngine.currentTimeMs)
        rawProgressMs.value = ms
        currentTime.value = duration.value > 0 ? Math.min(ms, duration.value) : ms
      }
      progressAnimationFrame = window.requestAnimationFrame(syncLocalProgress)
      return
    }

    const canRun = isPlaying.value && !isSeeking.value && !isLoading.value && !isBuffering.value
    if (canRun) {
      const elapsedMs = rafTimestamp - lastSyncedAt
      const estimatedTime = Math.round(lastSyncedProgressMs + elapsedMs)
      currentTime.value =
        duration.value > 0 ? Math.min(estimatedTime, duration.value) : estimatedTime
    } else if (_syncLocalLogCounter++ % 60 === 0) {
      // 每秒打一次日志（约60帧），避免刷屏
      console.log(
        `[SYNC] syncLocalProgress BLOCKED: isPlaying=${isPlaying.value}, isSeeking=${isSeeking.value}, isLoading=${isLoading.value}, currentTime=${currentTime.value}`
      )
    }
    progressAnimationFrame = window.requestAnimationFrame(syncLocalProgress)
  }

  /**
   * 启动进度同步计时器
   */
  const startTimer = (): void => {
    if (isWebEngine.value) {
      // WebAPI 引擎不轮询 native 后端，只用 rAF 读取 <audio>.currentTime
      if (!progressAnimationFrame) {
        progressAnimationFrame = window.requestAnimationFrame(syncLocalProgress)
      }
      if (!progressTimer) {
        progressTimer = setInterval(() => {
          void Promise.all([updateCacheProgress(), syncSongCachePlaybackPosition()])
        }, CACHE_PROGRESS_POLL_INTERVAL_MS)
        void Promise.all([updateCacheProgress(), syncSongCachePlaybackPosition()])
      }
      return
    }
    lastSyncedProgressMs = currentTime.value
    lastSyncedAt = performance.now()
    if (!progressAnimationFrame) {
      progressAnimationFrame = window.requestAnimationFrame(syncLocalProgress)
    }
    if (progressTimer) return
    progressTimer = setInterval(syncProgress, NATIVE_PROGRESS_POLL_INTERVAL_MS)
    void syncProgress()
  }

  /**
   * 停止进度同步计时器
   */
  const stopTimer = (): void => {
    if (progressTimer) {
      clearInterval(progressTimer)
      progressTimer = null
    }
    if (progressAnimationFrame) {
      window.cancelAnimationFrame(progressAnimationFrame)
      progressAnimationFrame = null
    }
  }

  /**
   * 获取歌曲详情及其权限
   */
  const getSongDetailData = async (id: number): Promise<{ song: Song } | undefined> => {
    try {
      const res = (await window.api.song_detail({ ids: [id] })) as {
        body?: { songs: Song[] }
      }
      if (res.body?.songs?.[0]) {
        return {
          song: res.body.songs[0]
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
  const fetchSongUrl = async (
    song_id: number,
    level: SoundQualityType
  ): Promise<{ url: string; level: SoundQualityType; size?: number; sampleRate?: number }> => {
    const levels = getFallbackQualities(level)
    for (const tryLevel of levels) {
      try {
        const res = (await window.api.song_url({
          id: song_id,
          level: tryLevel,
          cookie: userStore.cookie
        })) as {
          status: number
          body?: { data?: SongUrl[] } | null
          error?: string
          cookie?: unknown
        }
        const data = res.body?.data?.[0]
        const url = data?.url
        if (url) {
          if (tryLevel !== level) {
            console.warn(
              `[playback] 请求的音质 ${level} 不可用，已降级为 ${tryLevel}: song_id=${song_id}`
            )
          }
          return {
            url,
            level: isSoundQualityLevel(data?.level) ? data.level! : tryLevel,
            ...(typeof data?.size === 'number' && Number.isFinite(data.size)
              ? { size: data.size }
              : {}),
            ...(typeof data?.sr === 'number' && Number.isFinite(data.sr)
              ? { sampleRate: data.sr }
              : {})
          }
        }
        console.warn(
          `[playback] song_url 音质 ${tryLevel} 不可用: id=${song_id}, ` +
            `status=${res.status}, hasBody=${res.body !== null && res.body !== undefined}, ` +
            `dataLen=${JSON.stringify(res.body?.data?.length)}, ` +
            `error=${JSON.stringify(res.error)}, rawData=${JSON.stringify(data)}`
        )
      } catch (e) {
        console.warn(`[playback] song_url 音质 ${tryLevel} 请求异常:`, e)
      }
    }
    console.error(`[playback] 所有音质均不可用: song_id=${song_id}`)
    return { url: '', level }
  }

  const prepareCacheForPlayback = (
    engine: PlaybackCacheEngine,
    songId: number,
    songUrl: Awaited<ReturnType<typeof fetchSongUrl>>,
    durationMs: number
  ): Promise<PreparedPlaybackCache> => {
    return preparePlaybackCache({
      engine,
      songId,
      quality: songUrl.level,
      url: songUrl.url,
      expectedBytes: songUrl.size,
      durationMs
    })
  }

  const activatePlaybackCache = async (
    cache: PreparedPlaybackCache,
    songId: number
  ): Promise<void> => {
    const playbackSource = cache.source
    console.log(
      `[playback] preparePlaybackCache result: engine=${cache.engine}, type=${playbackSource.type}, value=${playbackSource.value}`
    )
    await cancelActiveSongCacheDownload()
    activeCacheMetadataPath = cache.metadataPath
    lastCachePlaybackPositionMs = -1
    bufferedPercent.value = cache.initialBufferedPercent
    await updateCacheProgress()

    const backgroundCache = startPlaybackBackgroundCache(cache)
    if (!backgroundCache) return

    console.log(`[playback] cache miss, starting background cache for song ${songId}`)
    backgroundCache
      .then((cached) => {
        if (cached.type === 'file') {
          console.log(`[playback] background cache completed: ${cached.value}`)
        }
      })
      .catch((err) => console.warn('后台缓存歌曲失败:', err))
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

      const latestProgress = await window.api.get_progress().catch(() => currentTime.value)
      const finishedNearEnd =
        duration.value <= 0 ||
        latestProgress >= Math.max(0, duration.value - NATURAL_END_TOLERANCE_MS)

      if (!finishedNearEnd) {
        await stopAfterPlaybackFailure('播放中断，可能是网络连接不稳定。')
        return
      }

      // 播放结束后的状态清理
      isPlaying.value = false
      isHistorySong.value = true
      resetPlaybackLoadState()
      stopTimer()
      currentTime.value = duration.value
      bufferedPercent.value = 100

      // 自动播放下一首
      await playNext(true)
    } catch {
      // 播放中断或 API 调用失败，通常不处理
    }
  }

  // --- 公开操作 (Actions) ---

  const isWebEngine = computed(() => configStore.audioEngine === 'webapi')

  /**
   * WebAPI 引擎播放：作为 native 引擎的对照路径，底层走 <audio> + 浏览器内置缓冲。
   * 复用缓存系统，优先播放已缓存的本地文件；未命中缓存时由浏览器拉取网络 URL，
   * 同时后台启动边播边存。
   */
  const playWithWebEngine = async (song_id: number, startTime: number): Promise<void> => {
    playToken++
    const currentToken = playToken
    isSwitching.value = true
    beginPlaybackLoadState(startTime)
    bufferedPercent.value = 0
    await cancelActiveSongCacheDownload()

    try {
      // 关掉 native 输出，避免两套引擎同时出声
      try {
        await window.api.stop()
      } catch {
        // ignore
      }

      const detailData = await getSongDetailData(song_id)
      if (currentToken !== playToken) return
      if (!detailData) throw new Error('无法获取歌曲详情')

      currentTime.value = startTime
      setPlayerData(detailData.song, false)

      const songUrl = await fetchSongUrl(song_id, configStore.soundQuality)
      if (currentToken !== playToken) return
      if (!songUrl.url) {
        console.warn(
          `[playback] fetchSongUrl 返回空 URL: song_id=${song_id}, level=${configStore.soundQuality}, cookie=${userStore.cookie ? '已设置' : '未设置（未登录）'}`
        )
        throw new Error('获取播放 URL 失败')
      }

      const playbackCache = await prepareCacheForPlayback(
        'webapi',
        song_id,
        songUrl,
        detailData.song.dt
      )
      if (currentToken !== playToken) return
      await activatePlaybackCache(playbackCache, song_id)

      await webAudioEngine.load(playbackCache.source, startTime / 1000)
      if (currentToken !== playToken) {
        webAudioEngine.stop()
        return
      }

      setPlayerData(detailData.song, true)
      isHistorySong.value = false
      currentTime.value = startTime
      resetPlaybackLoadState()

      const exists = playlistStore.playlist.some((s) => s.id === song_id)
      if (!exists) {
        playlistStore.addToPlaylist({
          id: detailData.song.id,
          name: detailData.song.name,
          artists: createCurrentSongArtists(detailData.song.ar),
          cover: detailData.song.al.picUrl,
          duration: detailData.song.dt
        })
      }
    } catch (error) {
      if (currentToken === playToken) {
        console.error('WebAPI 播放失败:', error)
        playbackError.value = error instanceof Error ? error.message : '音乐播放失败。'
        webAudioEngine.stop()
        isPlaying.value = false
        isHistorySong.value = true
        resetPlaybackLoadState()
        stopTimer()
        await cancelActiveSongCacheDownload()
      }
    } finally {
      if (currentToken === playToken) {
        isSwitching.value = false
      }
    }
  }

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

    // WebAPI 引擎走完全独立的播放路径
    if (isWebEngine.value) {
      await playWithWebEngine(song_id, startTime)
      return
    }

    console.log(`开始播放歌曲 ID ${song_id}，起始时间 ${startTime}ms，强制重启: ${forceRestart}`)

    // 2. 准备开始切换流程
    // 允许新任务中断正在进行的旧任务。我们通过 playToken 来标识每个任务的唯一性。
    playToken++
    const currentToken = playToken
    isSwitching.value = true
    beginPlaybackLoadState(startTime)
    bufferedPercent.value = 0
    await cancelActiveSongCacheDownload()

    try {
      // 3. 继续播放逻辑：如果是同一首歌但处于暂停状态，且非历史回放，直接恢复播放
      if (
        !forceRestart &&
        currentSongId.value === song_id &&
        !isPlaying.value &&
        !isHistorySong.value
      ) {
        await withTimeout(
          configStore.ensureConfiguredOutputDevice(),
          PLAYBACK_OPERATION_TIMEOUT_MS,
          '音频设备准备超时'
        )
        await withTimeout(window.api.resume(), PLAYBACK_OPERATION_TIMEOUT_MS, '恢复播放超时')
        if (currentToken === playToken) {
          isPlaying.value = true
          resetPlaybackLoadState()
        }
        return
      }

      // 4. 获取歌曲详情：包含歌曲信息和播放权限
      const detailData = await withTimeout(
        getSongDetailData(song_id),
        PLAYBACK_OPERATION_TIMEOUT_MS,
        '获取歌曲详情超时'
      )
      // 在每一个异步步长后，都要检查任务是否已过期
      if (currentToken !== playToken) return

      if (!detailData) {
        throw new Error('无法获取歌曲详情')
      }

      const { song } = detailData
      currentTime.value = startTime
      lastSyncedProgressMs = startTime
      lastSyncedAt = performance.now()
      setPlayerData(song, false)

      // 5. 按用户设置请求音质。真实可播放音质以 song_url_v1 返回的 data[0].level 为准。
      const targetLevel = configStore.soundQuality

      // 6. 获取歌曲播放地址 (URL)
      const songUrl = await withTimeout(
        fetchSongUrl(song_id, targetLevel),
        PLAYBACK_OPERATION_TIMEOUT_MS,
        '获取播放 URL 超时'
      )
      if (currentToken !== playToken) return
      if (!songUrl.url) {
        console.warn(
          `[playback] fetchSongUrl 返回空 URL: song_id=${song_id}, level=${targetLevel}, cookie=${userStore.cookie ? '已设置' : '未设置（未登录）'}`
        )
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
      await withTimeout(
        configStore.ensureConfiguredOutputDevice(),
        PLAYBACK_OPERATION_TIMEOUT_MS,
        '音频设备准备超时'
      )
      if (currentToken !== playToken) return

      // 9. 准备播放源：处理缓存逻辑 (可能是本地文件，也可能是带缓存的 URL)
      const playbackCache = await withTimeout(
        prepareCacheForPlayback('native', song_id, songUrl, song.dt),
        PLAYBACK_OPERATION_TIMEOUT_MS,
        '准备播放缓存超时'
      )
      if (currentToken !== playToken) return
      await activatePlaybackCache(playbackCache, song_id)

      // 10. 执行底层播放指令
      const playbackSource = playbackCache.source
      const startTimeInSeconds = startTime / 1000
      if (playbackSource.type === 'file') {
        // 情况 A: 命中本地文件缓存，直接播放文件
        await withTimeout(
          window.api.play_file(
            playbackSource.value,
            startTimeInSeconds,
            configStore.strictBitPerfect
          ),
          PLAYBACK_OPERATION_TIMEOUT_MS,
          '播放本地缓存超时'
        )
      } else if (playbackSource.cachePath && playbackSource.metadataPath) {
        // 情况 B: 支持边播边存，调用带缓存功能的播放接口
        await withTimeout(
          window.api.play_url_cached(
            playbackSource.value,
            playbackSource.cachePath,
            playbackSource.metadataPath,
            song.dt,
            playbackSource.cacheAheadSecs ?? 30,
            playbackSource.maxCacheAheadBytes ?? configStore.songMaxCacheAheadBytes,
            startTimeInSeconds,
            configStore.strictBitPerfect
          ),
          PLAYBACK_OPERATION_TIMEOUT_MS,
          '网络播放启动超时'
        )
      } else {
        // 情况 C: 普通网络 URL 播放
        await withTimeout(
          window.api.play_url(
            playbackSource.value,
            startTimeInSeconds,
            configStore.strictBitPerfect
          ),
          PLAYBACK_OPERATION_TIMEOUT_MS,
          '网络播放启动超时'
        )
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
        const message = error instanceof Error ? error.message : '音乐播放失败。'
        await stopAfterPlaybackFailure(message)
        if (message.includes('BitPerfect')) {
          void dialogStore.open({
            title: '无法满足 BitPerfect',
            message,
            confirmText: '确定'
          })
        }
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
    if (isSwitching.value || isLoading.value) return

    if (isWebEngine.value) {
      if (isPlaying.value) {
        webAudioEngine.pause()
        isPlaying.value = false
        return
      }
      if (isHistorySong.value && currentSongId.value) {
        await playMusic(currentSongId.value, currentTime.value)
        return
      }
      await webAudioEngine.resume()
      isPlaying.value = true
      return
    }

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
    const seekStartedAt = performance.now()
    const finiteTime = Number.isFinite(timeInMs) ? timeInMs : 0
    const roundedTime = Math.max(Math.round(finiteTime), 0)
    const clampedTime = duration.value > 0 ? Math.min(roundedTime, duration.value) : roundedTime

    if (isWebEngine.value) {
      isBuffering.value = true
      webAudioEngine.seek(clampedTime / 1000)
      currentTime.value = clampedTime
      rawProgressMs.value = clampedTime
      isSeeking.value = false
      void syncSongCachePlaybackPosition(clampedTime)
      return
    }

    console.log(
      `[SEEK] START: target=${clampedTime}ms, isSeeking=${isSeeking.value}, isPlaying=${isPlaying.value}`
    )
    currentTime.value = clampedTime
    rawProgressMs.value = clampedTime
    lastSyncedProgressMs = clampedTime
    lastSyncedAt = performance.now()
    // 给后端1秒钟的时间来 flush 缓冲区并更新真实 progressMs。
    // 在这1秒内，忽略任何来自 get_progress() 的旧数据造成的大幅漂移，防止 UI 回弹。
    ignoreDriftUntil = performance.now() + 1000
    // 注意：不调用 beginPlaybackLoadState()。
    // isLoading 是为初始加载设计的——seek 时我们已经知道目标位置，
    // currentTime 已设好，rAF 插值应该从目标位置立即继续推进，
    // 而不是等后端缓冲区追上（~1秒延迟）。
    playbackError.value = ''
    await window.api.seek(clampedTime / 1000)
    console.log(
      `[SEEK] API RETURNED: took ${(performance.now() - seekStartedAt).toFixed(0)}ms, isSeeking=${isSeeking.value}, isLoading=${isLoading.value}`
    )
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
    if (isWebEngine.value) {
      webAudioEngine.stop()
    } else {
      try {
        await window.api.stop()
      } catch {
        // ignore
      }
    }
    currentSong.value = null
    currentSongId.value = null
    isPlaying.value = false
    currentTime.value = 0
    isHistorySong.value = true
    playbackError.value = ''
    bufferedPercent.value = 0
    isBuffering.value = false
    rawProgressMs.value = 0
    lastSyncedProgressMs = 0
    lastSyncedAt = performance.now()
    ignoreDriftUntil = 0
    await cancelActiveSongCacheDownload()
    resetPlaybackLoadState()
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
    const currentTimeMs = Math.floor(time)
    const shouldPersist =
      currentTimeMs === 0 ||
      currentTimeMs === duration.value ||
      Math.abs(currentTimeMs - lastPersistedCurrentTime) >= 1000

    if (!shouldPersist) return

    lastPersistedCurrentTime = currentTimeMs
    localStorage.setItem('currentTime', currentTimeMs.toString())
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
    () => configStore.audioEngine,
    (newEngine, oldEngine) => {
      if (oldEngine === undefined) return
      if (newEngine === oldEngine) return
      if (!currentSongId.value || isSwitching.value) return

      const songId = currentSongId.value
      const savedTime = currentTime.value
      stopTimer()
      webAudioEngine.stop()
      try {
        window.api.stop().catch(() => {})
      } catch {
        // ignore
      }
      void (async () => {
        await cancelActiveSongCacheDownload()
        await playMusic(songId, savedTime)
      })()
    }
  )

  // WebAPI 引擎的事件回调（自然结束自动下一首、缓冲/播放态同步、错误处理）
  webAudioEngine.setCallbacks({
    onEnded: () => {
      if (!isWebEngine.value) return
      isPlaying.value = false
      isHistorySong.value = true
      resetPlaybackLoadState()
      stopTimer()
      currentTime.value = duration.value
      bufferedPercent.value = 100
      void (async () => {
        await syncSongCachePlaybackPosition(duration.value)
        await playNext(true)
      })()
    },
    onBuffering: (buffering) => {
      if (isWebEngine.value) isBuffering.value = buffering
    },
    onPlayStateChange: (playing) => {
      // 反映浏览器侧的播放/暂停（如系统媒体键），切换加载期间不干预
      if (isWebEngine.value && !isSwitching.value) isPlaying.value = playing
    },
    onError: (message) => {
      if (!isWebEngine.value) return
      playbackError.value = message
      isPlaying.value = false
      isHistorySong.value = true
      resetPlaybackLoadState()
      stopTimer()
      void cancelActiveSongCacheDownload()
    }
  })

  return {
    currentSong,
    currentSongId,
    currentTime,
    isPlaying,
    isSeeking,
    isBuffering,
    rawProgressMs,
    isLoading,
    playbackError,
    bufferedPercent,
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
