import { createPinia, setActivePinia } from 'pinia'
import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest'
import { useConfigStore } from '../configStore'
import { usePlaybackStore } from '../player/playback'
import { usePlayerStore } from '../playerStore'

class MemoryStorage implements Storage {
  private readonly store = new Map<string, string>()

  get length(): number {
    return this.store.size
  }

  clear(): void {
    this.store.clear()
  }

  getItem(key: string): string | null {
    return this.store.get(key) ?? null
  }

  key(index: number): string | null {
    return Array.from(this.store.keys())[index] ?? null
  }

  removeItem(key: string): void {
    this.store.delete(key)
  }

  setItem(key: string, value: string): void {
    this.store.set(key, value)
  }
}

const SETTINGS_KEY = 'app_settings'

describe('playerStore device switch sequencing', () => {
  let storage: MemoryStorage

  beforeEach(() => {
    storage = new MemoryStorage()
    vi.stubGlobal('localStorage', storage)
    vi.stubGlobal('document', { cookie: '' } as Document)
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    vi.spyOn(console, 'warn').mockImplementation(() => undefined)
    setActivePinia(createPinia())
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('stops the previous playback before trying to reacquire the configured output device for a new song', async () => {
    storage.setItem(
      SETTINGS_KEY,
      JSON.stringify({
        outputDeviceId: 'headphones',
        soundQuality: 'standard'
      })
    )

    let playbackActive = true
    let currentDeviceId = 'default'
    const callOrder: string[] = []

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: vi.fn(async ({ ids }: { ids: number[] }) => ({
          body: {
            songs: [
              {
                id: ids[0],
                name: `Song ${ids[0]}`,
                dt: 180000,
                ar: [{ id: 1, name: 'Artist' }],
                al: { id: 1, name: 'Album', picUrl: 'cover' },
                h: null,
                sq: null,
                hr: null
              }
            ],
            privileges: [
              {
                id: ids[0],
                playMaxBrLevel: 'standard',
                chargeInfoList: []
              }
            ]
          }
        })),
        song_url: vi.fn(async () => ({
          body: {
            data: [{ url: 'https://example.com/test.mp3' }]
          }
        })),
        get_output_devices: vi.fn(async () => [
          {
            id: 'headphones',
            name: 'Headphones',
            isDefault: false,
            isCurrent: currentDeviceId === 'headphones'
          },
          {
            id: 'default',
            name: 'System Default',
            isDefault: true,
            isCurrent: currentDeviceId === 'default'
          }
        ]),
        switch_output_device: vi.fn(async (deviceId?: string) => {
          callOrder.push(`switch:${deviceId ?? 'default'}`)

          if (playbackActive) {
            throw new Error('device still busy')
          }

          currentDeviceId = deviceId ?? 'default'
        }),
        stop: vi.fn(async () => {
          callOrder.push('stop')
          playbackActive = false
        }),
        pause: vi.fn(async () => {
          callOrder.push('pause')
        }),
        play_url: vi.fn(async () => {
          callOrder.push('play_url')
          playbackActive = true
        }),
        resume: vi.fn(async () => undefined),
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        seek: vi.fn(async () => undefined),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const configStore = useConfigStore()
    const playerStore = usePlayerStore()

    playerStore.currentSongId = 1
    playerStore.currentSong = {
      id: 1,
      name: 'Current Song',
      artists: [{ id: 1, name: 'Artist' }],
      cover: 'cover',
      duration: 180000
    }
    playerStore.isPlaying = true

    await playerStore.playMusic(2)

    expect(callOrder).toEqual(['stop', 'switch:headphones', 'play_url'])
    expect(configStore.outputDeviceId).toBe('headphones')
    expect(currentDeviceId).toBe('headphones')
  })

  it('keeps native seek out of loading and ignores stale progress during drift immunity', async () => {
    let now = 1_000
    let progressTimer: (() => void) | undefined
    // 模拟 seek 后底层仍停留在旧位置(10s)，且 buffering 为 true
    const getProgress = vi.fn(async () => 10_000)

    vi.spyOn(performance, 'now').mockImplementation(() => now)
    vi.stubGlobal(
      'setInterval',
      vi.fn((callback: () => void) => {
        progressTimer = callback
        return 1 as unknown as ReturnType<typeof setInterval>
      })
    )
    vi.stubGlobal('clearInterval', vi.fn())
    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        get_progress: getProgress,
        is_buffering: vi.fn(async () => true),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        seek: vi.fn(async () => undefined)
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()
    playerStore.currentSong = {
      id: 1,
      name: 'Current Song',
      artists: [{ id: 1, name: 'Artist' }],
      cover: 'cover',
      duration: 180000
    }
    playerStore.isPlaying = true

    await playerStore.seek(60_000)

    // seek 不进入启动加载态；1 秒免疫窗口内忽略旧进度，避免 UI 回跳
    expect(playerStore.isLoading).toBe(false)
    now = 1_500
    await progressTimer?.()

    expect(getProgress).toHaveBeenCalled()
    expect(playerStore.currentTime).toBe(60_000)
    expect(playerStore.isLoading).toBe(false)
  })

  it('corrects to native progress after seek drift immunity expires', async () => {
    let now = 1_000
    let progressTimer: (() => void) | undefined
    let nativeProgress = 0
    let buffering = false
    const getProgress = vi.fn(async () => nativeProgress)

    vi.spyOn(performance, 'now').mockImplementation(() => now)
    vi.stubGlobal(
      'setInterval',
      vi.fn((callback: () => void) => {
        progressTimer = callback
        return 1 as unknown as ReturnType<typeof setInterval>
      })
    )
    vi.stubGlobal('clearInterval', vi.fn())
    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        get_progress: getProgress,
        is_buffering: vi.fn(async () => buffering),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        seek: vi.fn(async () => undefined)
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()
    playerStore.currentSong = {
      id: 1,
      name: 'Current Song',
      artists: [{ id: 1, name: 'Artist' }],
      cover: 'cover',
      duration: 180000
    }
    playerStore.isPlaying = true

    await playerStore.seek(60_000)

    // 免疫窗口结束后，底层已在目标点恢复出声：重新按后端真实进度校正
    nativeProgress = 60_000
    buffering = false
    now = 2_100
    await progressTimer?.()

    expect(playerStore.isLoading).toBe(false)
    expect(playerStore.currentTime).toBe(60_000)

    // 之后随后端进度推进而平滑跟踪
    nativeProgress = 61_000
    now = 2_300
    await progressTimer?.()

    expect(playerStore.currentTime).toBe(61_000)
  })

  it('requests configured quality and uses the API returned actual level for cache identity', async () => {
    storage.setItem(
      SETTINGS_KEY,
      JSON.stringify({
        soundQuality: 'hires'
      })
    )

    const songUrl = vi.fn(async () => ({
      body: {
        data: [{ url: 'https://example.com/lossless.flac', level: 'lossless' }]
      }
    }))
    const prepareCachedSongSource = vi.fn(async () => ({
      type: 'url',
      value: 'https://example.com/lossless.flac'
    }))

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: vi.fn(async ({ ids }: { ids: number[] }) => ({
          body: {
            songs: [
              {
                id: ids[0],
                name: `Song ${ids[0]}`,
                dt: 180000,
                ar: [{ id: 1, name: 'Artist' }],
                al: { id: 1, name: 'Album', picUrl: 'cover' },
                h: null,
                sq: null,
                hr: null
              }
            ],
            privileges: [
              {
                id: ids[0],
                playMaxBrLevel: 'standard',
                maxBrLevel: 'standard',
                plLevel: 'standard',
                chargeInfoList: []
              }
            ]
          }
        })),
        song_url: songUrl,
        get_output_devices: vi.fn(async () => [
          {
            id: 'default',
            name: 'System Default',
            isDefault: true,
            isCurrent: true
          }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_url: vi.fn(async () => undefined),
        prepare_cached_song_source: prepareCachedSongSource,
        resume: vi.fn(async () => undefined),
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        seek: vi.fn(async () => undefined),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()

    await playerStore.playMusic(1)

    expect(songUrl).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 1,
        level: 'hires'
      })
    )
    expect(prepareCachedSongSource).toHaveBeenCalledWith(
      expect.objectContaining({
        songId: 1,
        quality: 'lossless',
        url: 'https://example.com/lossless.flac'
      })
    )
  })

  it('keeps native playback independent from the position-limited background cache', async () => {
    vi.spyOn(performance, 'now').mockReturnValue(1_000)
    vi.stubGlobal(
      'setInterval',
      vi.fn(() => 1 as unknown as ReturnType<typeof setInterval>)
    )
    vi.stubGlobal('clearInterval', vi.fn())

    const playUrl = vi.fn(async () => undefined)
    const playUrlCached = vi.fn(async () => undefined)
    const cacheSongSource = vi.fn(() => new Promise(() => undefined))
    const updatePlaybackPosition = vi.fn(async () => true)

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: vi.fn(async ({ ids }: { ids: number[] }) => ({
          body: {
            songs: [
              {
                id: ids[0],
                name: `Song ${ids[0]}`,
                dt: 180000,
                ar: [{ id: 1, name: 'Artist' }],
                al: { id: 1, name: 'Album', picUrl: 'cover' },
                h: null,
                sq: null,
                hr: null
              }
            ]
          }
        })),
        song_url: vi.fn(async () => ({
          body: {
            data: [
              {
                url: 'https://example.com/test.flac',
                level: 'standard',
                size: 3 * 1024 * 1024
              }
            ]
          }
        })),
        get_output_devices: vi.fn(async () => [
          {
            id: 'default',
            name: 'System Default',
            isDefault: true,
            isCurrent: true
          }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_url: playUrl,
        play_url_cached: playUrlCached,
        prepare_cached_song_source: vi.fn(async () => ({
          type: 'url',
          value: 'https://example.com/test.flac',
          cachePath: '/tmp/test.flac',
          metadataPath: '/tmp/test.flac.meta.json'
        })),
        cache_song_source: cacheSongSource,
        get_progress: vi.fn(async () => 12_000),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 10 })),
        update_song_cache_playback_position: updatePlaybackPosition,
        cancel_song_cache_download: vi.fn(async () => true),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()
    await playerStore.playMusic(1)
    await Promise.resolve()
    await Promise.resolve()
    await Promise.resolve()

    expect(playUrl).toHaveBeenCalledWith('https://example.com/test.flac', 0, false)
    expect(playUrlCached).not.toHaveBeenCalled()
    expect(cacheSongSource).toHaveBeenCalledWith({
      songId: 1,
      quality: 'standard',
      url: 'https://example.com/test.flac',
      expectedBytes: 3 * 1024 * 1024,
      durationMs: 180000
    })
    expect(updatePlaybackPosition).toHaveBeenCalledWith({
      metadataPath: '/tmp/test.flac.meta.json',
      playbackPositionMs: 12_000
    })
  })

  it('starts native history playback from the persisted progress instead of zero', async () => {
    storage.setItem('currentTime', '42000')
    storage.setItem('currentSongId', '1')
    storage.setItem(
      'currentSong',
      JSON.stringify({
        id: 1,
        name: 'Stored Song',
        artists: [{ id: 1, name: 'Artist' }],
        cover: 'cover',
        duration: 180000
      })
    )

    const playFile = vi.fn(async () => undefined)

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: vi.fn(async ({ ids }: { ids: number[] }) => ({
          body: {
            songs: [
              {
                id: ids[0],
                name: `Song ${ids[0]}`,
                dt: 180000,
                ar: [{ id: 1, name: 'Artist' }],
                al: { id: 1, name: 'Album', picUrl: 'cover' },
                h: null,
                sq: null,
                hr: null
              }
            ]
          }
        })),
        song_url: vi.fn(async () => ({
          body: {
            data: [{ url: 'https://example.com/test.mp3', level: 'standard' }]
          }
        })),
        get_output_devices: vi.fn(async () => [
          {
            id: 'default',
            name: 'System Default',
            isDefault: true,
            isCurrent: true
          }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_url: vi.fn(async () => undefined),
        play_file: playFile,
        prepare_cached_song_source: vi.fn(async () => ({
          type: 'file',
          value: '/tmp/cached-test.mp3'
        })),
        resume: vi.fn(async () => undefined),
        get_progress: vi.fn(async () => 42_000),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        seek: vi.fn(async () => undefined),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()

    expect(playerStore.currentTime).toBe(42_000)

    await playerStore.togglePlay()

    expect(playFile).toHaveBeenCalledWith('/tmp/cached-test.mp3', 42, false)
    expect(playerStore.currentTime).toBe(42_000)
  })

  it('keeps startup loading until native progress has actually advanced', async () => {
    let now = 1_000
    let animationFrameCallback: FrameRequestCallback | undefined

    vi.spyOn(performance, 'now').mockImplementation(() => now)
    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn((callback: FrameRequestCallback) => {
        animationFrameCallback = callback
        return 1
      }),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: vi.fn(async ({ ids }: { ids: number[] }) => ({
          body: {
            songs: [
              {
                id: ids[0],
                name: `Song ${ids[0]}`,
                dt: 180000,
                ar: [{ id: 1, name: 'Artist' }],
                al: { id: 1, name: 'Album', picUrl: 'cover' },
                h: null,
                sq: null,
                hr: null
              }
            ]
          }
        })),
        song_url: vi.fn(async () => ({
          body: {
            data: [{ url: 'https://example.com/test.mp3', level: 'standard' }]
          }
        })),
        get_output_devices: vi.fn(async () => [
          {
            id: 'default',
            name: 'System Default',
            isDefault: true,
            isCurrent: true
          }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_url: vi.fn(async () => undefined),
        resume: vi.fn(async () => undefined),
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        seek: vi.fn(async () => undefined),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()

    await playerStore.playMusic(1)
    await Promise.resolve()
    await Promise.resolve()

    expect(playerStore.isLoading).toBe(true)

    now = 2_000
    animationFrameCallback?.(now)

    expect(playerStore.currentTime).toBe(0)
  })

  it('shows loading immediately for a new network song and keeps cancel non-blocking', async () => {
    let cancelCalls = 0
    let resolveStuckCancel: ((value: boolean) => void) | undefined
    const cancelStarted = vi.fn()
    const songDetail = vi.fn(async ({ ids }: { ids: number[] }) => ({
      body: {
        songs: [
          {
            id: ids[0],
            name: `Song ${ids[0]}`,
            dt: 180000,
            ar: [{ id: 1, name: 'Artist' }],
            al: { id: 1, name: 'Album', picUrl: 'cover' },
            h: null,
            sq: null,
            hr: null
          }
        ]
      }
    }))
    const songUrl = vi.fn(async ({ id }: { id: number }) => ({
      body: {
        data: [
          {
            url: `https://example.com/${id}.mp3`,
            level: 'standard',
            size: 1024
          }
        ]
      }
    }))
    const playUrl = vi.fn(async () => undefined)

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: songDetail,
        song_url: songUrl,
        get_output_devices: vi.fn(async () => [
          {
            id: 'default',
            name: 'System Default',
            isDefault: true,
            isCurrent: true
          }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_url: playUrl,
        prepare_cached_song_source: vi.fn(async ({ songId }: { songId: number }) => ({
          type: 'url',
          value: `https://example.com/${songId}.mp3`,
          cachePath: `/tmp/${songId}.cache`,
          metadataPath: `/tmp/${songId}.meta.json`
        })),
        cache_song_source: vi.fn(async ({ songId }: { songId: number }) => ({
          type: 'url',
          value: `https://example.com/${songId}.mp3`,
          metadataPath: `/tmp/${songId}.meta.json`
        })),
        cancel_song_cache_download: vi.fn(async (metadataPath: string) => {
          cancelCalls += 1
          cancelStarted(metadataPath)
          // 切走第一首时模拟 cancel 卡住（旧逻辑会拖死整条 playMusic）
          if (String(metadataPath).includes('/1.meta.json')) {
            return await new Promise<boolean>((resolve) => {
              resolveStuckCancel = resolve
            })
          }
          return true
        }),
        update_song_cache_playback_position: vi.fn(async () => true),
        get_progress: vi.fn(async () => 1_000),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 10 })),
        seek: vi.fn(async () => undefined),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()

    // 先播第一首，建立 activeCacheMetadataPath
    await playerStore.playMusic(1, 0, true)
    await Promise.resolve()
    await Promise.resolve()
    expect(playerStore.isLoading).toBe(true)

    // 再切第二首：应立刻进入 loading，cancel 卡住也不能拖死整条 playMusic
    const playNextPromise = playerStore.playMusic(2, 0, true)
    await Promise.resolve()
    await Promise.resolve()
    expect(playerStore.isLoading).toBe(true)

    // 放行卡住的 cancel（或等前端 1.5s 超时兜底）
    resolveStuckCancel?.(true)
    await playNextPromise
    await Promise.resolve()
    await Promise.resolve()

    expect(songDetail).toHaveBeenCalled()
    expect(songUrl).toHaveBeenCalled()
    expect(playUrl).toHaveBeenCalled()
    // 关键是 cancel 卡住也不会阻断新歌启动；loading 会在后续 progress 同步后清除
    expect(cancelStarted).toHaveBeenCalled()
  })

  it('allows pause while loading so the play button is not dead during stuck startup', async () => {
    const pause = vi.fn(async () => undefined)

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        pause,
        resume: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        get_progress: vi.fn(async () => 1_000),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        seek: vi.fn(async () => undefined),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()
    const playbackStore = usePlaybackStore()
    playerStore.isPlaying = true
    playbackStore.isLoading = true
    playerStore.currentSongId = 99

    await playerStore.togglePlay()

    expect(pause).toHaveBeenCalled()
    expect(playerStore.isPlaying).toBe(false)
  })

  it('retries an unaccepted WebAPI cache position without requiring currentTime to advance', async () => {
    storage.setItem(
      SETTINGS_KEY,
      JSON.stringify({
        audioEngine: 'webapi',
        soundQuality: 'standard'
      })
    )

    let progressTimer: (() => void) | undefined
    const updatePlaybackPosition = vi
      .fn<() => Promise<boolean>>()
      .mockResolvedValueOnce(false)
      .mockResolvedValue(true)

    class FakeAudio {
      preload = ''
      src = ''
      currentTime = 12
      duration = 180
      paused = true
      readyState = 1
      error: MediaError | null = null
      private readonly listeners = new Map<string, Array<() => void>>()

      addEventListener(name: string, callback: () => void): void {
        const callbacks = this.listeners.get(name) ?? []
        callbacks.push(callback)
        this.listeners.set(name, callbacks)
      }

      load(): void {
        // No media loading is needed for this timer-focused test.
      }

      async play(): Promise<void> {
        this.paused = false
        this.listeners.get('play')?.forEach((callback) => callback())
      }

      pause(): void {
        this.paused = true
        this.listeners.get('pause')?.forEach((callback) => callback())
      }

      removeAttribute(): void {
        this.src = ''
      }
    }

    vi.stubGlobal('Audio', FakeAudio)
    vi.stubGlobal(
      'setInterval',
      vi.fn((callback: () => void) => {
        progressTimer = callback
        return 1 as unknown as ReturnType<typeof setInterval>
      })
    )
    vi.stubGlobal('clearInterval', vi.fn())
    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: vi.fn(async ({ ids }: { ids: number[] }) => ({
          body: {
            songs: [
              {
                id: ids[0],
                name: `Song ${ids[0]}`,
                dt: 180000,
                ar: [{ id: 1, name: 'Artist' }],
                al: { id: 1, name: 'Album', picUrl: 'cover' },
                h: null,
                sq: null,
                hr: null
              }
            ]
          }
        })),
        song_url: vi.fn(async () => ({
          body: {
            data: [
              {
                url: 'https://example.com/test.mp3',
                level: 'standard',
                size: 3 * 1024 * 1024
              }
            ]
          }
        })),
        stop: vi.fn(async () => undefined),
        prepare_cached_song_source: vi.fn(async () => ({
          type: 'url',
          value: 'https://example.com/test.mp3',
          cachePath: '/tmp/test.mp3',
          metadataPath: '/tmp/test.mp3.meta.json'
        })),
        cache_song_source: vi.fn(async () => ({
          type: 'url',
          value: 'https://example.com/test.mp3',
          cachePath: '/tmp/test.mp3',
          metadataPath: '/tmp/test.mp3.meta.json'
        })),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        update_song_cache_playback_position: updatePlaybackPosition,
        cancel_song_cache_download: vi.fn(async () => true)
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()
    await playerStore.playMusic(1)
    await Promise.resolve()
    await Promise.resolve()

    expect(updatePlaybackPosition).toHaveBeenCalledTimes(1)
    expect(updatePlaybackPosition).toHaveBeenLastCalledWith({
      metadataPath: '/tmp/test.mp3.meta.json',
      playbackPositionMs: 12_000
    })

    // 模拟 audio 进入 waiting 后 currentTime 固定在 12s；即使位置没有变化，
    // 前一次 native 返回 false 也必须再次发送，不能被 last-position 去重吞掉。
    await progressTimer?.()

    expect(updatePlaybackPosition).toHaveBeenCalledTimes(2)
    expect(updatePlaybackPosition).toHaveBeenLastCalledWith({
      metadataPath: '/tmp/test.mp3.meta.json',
      playbackPositionMs: 12_000
    })
  })

  it('plays a local queue item directly without requesting cloud song data', async () => {
    const songDetail = vi.fn()
    const playFile = vi.fn(async () => undefined)

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: songDetail,
        get_output_devices: vi.fn(async () => [
          {
            id: 'default',
            name: 'System Default',
            isDefault: true,
            isCurrent: true
          }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_file: playFile,
        get_progress: vi.fn(async () => 42_000),
        is_buffering: vi.fn(async () => false),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()
    const localSong = {
      id: -100,
      name: 'Local Track',
      artists: [{ id: 0, name: 'Local Artist' }],
      cover: '',
      duration: 180_000,
      source: 'local' as const,
      filePath: '/music/local-track.flac',
      fileName: 'local-track.flac'
    }

    await playerStore.playAll([localSong])

    expect(songDetail).not.toHaveBeenCalled()
    expect(playFile).toHaveBeenCalledWith('/music/local-track.flac', 0, false)
    expect(playerStore.currentSong).toEqual(localSong)
    expect(playerStore.isPlaying).toBe(true)
  })
})
