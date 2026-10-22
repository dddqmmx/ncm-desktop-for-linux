import { createPinia, setActivePinia } from 'pinia'
import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest'
import { useConfigStore } from '../configStore'
import { usePlaybackStore } from '../player/playback'
import { usePlayerStore } from '../playerStore'
import type { CurrentSong } from '@renderer/types/player'

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

    expect(playUrl).toHaveBeenCalledWith('https://example.com/test.flac', 0, false, false)
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

    expect(playFile).toHaveBeenCalledWith('/tmp/cached-test.mp3', 42, false, false)
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
    const getFileDuration = vi.fn(async () => 180_000)

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
        get_file_duration: getFileDuration,
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
      duration: 0,
      source: 'local' as const,
      filePath: '/music/local-track.flac',
      fileName: 'local-track.flac'
    }

    await playerStore.playAll([localSong])

    expect(songDetail).not.toHaveBeenCalled()
    expect(getFileDuration).toHaveBeenCalledWith('/music/local-track.flac')
    expect(playFile).toHaveBeenCalledWith('/music/local-track.flac', 0, false, false)
    expect(playerStore.currentSong).toEqual({ ...localSong, duration: 180_000 })
    expect(playerStore.duration).toBe(180_000)
    expect(playerStore.isPlaying).toBe(true)
  })

  it('fills the duration of a persisted local song during player initialization', async () => {
    storage.setItem('currentSongId', '-101')
    storage.setItem(
      'currentSong',
      JSON.stringify({
        id: -101,
        name: 'Stored Local Track',
        artists: [{ id: 0, name: 'Local Artist' }],
        cover: '',
        duration: 0,
        source: 'local',
        filePath: '/music/stored.flac',
        fileName: 'stored.flac'
      })
    )
    const getFileDuration = vi.fn(async () => 201_500)

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: { get_file_duration: getFileDuration }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()
    await playerStore.initFromStorage()

    expect(getFileDuration).toHaveBeenCalledWith('/music/stored.flac')
    expect(playerStore.currentSong?.duration).toBe(201_500)
    expect(JSON.parse(storage.getItem('currentSong') ?? '{}').duration).toBe(201_500)
  })

  // ====== 应用级软件音量：实时应用与失步自愈 ======

  /**
   * 开播前必须同步播种音量（不经 80ms 节流）：SetVolume 晚于 Play 入队的话，
   * 新会话带着旧增益开播、节流到期后又被中途调整——用户音量低于 100% 时
   * 每次切歌开头都是一声全音量爆响。
   */
  it('seeds the current volume synchronously before each native playback start', async () => {
    const setVolumeCalls: Array<{ at: string; volume: number }> = []
    const playCalls: string[] = []

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
          body: { data: [{ url: 'https://example.com/test.mp3' }] }
        })),
        get_output_devices: vi.fn(async () => []),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_url: vi.fn(async () => {
          playCalls.push('play')
        }),
        set_volume: vi.fn(async (volume: number) => {
          setVolumeCalls.push({ at: `after-${playCalls.length}-plays`, volume })
        }),
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 100, isComplete: true })),
        seek: vi.fn(async () => undefined),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const configStore = useConfigStore()
    configStore.softwareVolume = true
    configStore.volumePercent = 30
    // 放弃节流窗口：等 80ms 节流定时器跑完再开播
    await new Promise((resolve) => setTimeout(resolve, 100))

    const playerStore = usePlayerStore()
    await playerStore.playMusic(7)

    const playIndex = setVolumeCalls.findIndex((call) => call.at === 'after-0-plays')
    expect(playIndex).toBeGreaterThanOrEqual(0)
    expect(setVolumeCalls[playIndex]?.volume).toBeCloseTo(0.3, 5)
  })

  /**
   * 软件音量中途被关掉（或 BitPerfect 被打开）不得把正在播的会话打到全音量：
   * BitPerfect 是开播即冻结的选项，此刻会话还不是 BitPerfect，拉到 1.0 只是
   * 一声爆响，直通本身反而要到下一首才生效。
   */
  it('does not blast the current session to full volume when software volume is disabled mid-track', async () => {
    const setVolume = vi.fn(async () => undefined)

    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        set_volume: setVolume,
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 100, isComplete: true }))
      }
    } as unknown as Window & typeof globalThis)

    const configStore = useConfigStore()
    configStore.softwareVolume = true
    configStore.volumePercent = 40
    const playerStore = usePlayerStore()
    void playerStore
    // 放弃节流窗口：等 80ms 节流定时器跑完再切换
    await new Promise((resolve) => setTimeout(resolve, 100))
    expect(setVolume).toHaveBeenCalledWith(0.4)

    // 中途关掉软件音量：不立即下发 SetVolume(1.0)
    configStore.softwareVolume = false
    await new Promise((resolve) => setTimeout(resolve, 200))
    expect(setVolume).not.toHaveBeenCalledWith(1)

    // 重新打开：立即下发所选百分比（只会变安静，用户预期就是立即应用）
    configStore.softwareVolume = true
    await new Promise((resolve) => setTimeout(resolve, 200))
    expect(setVolume).toHaveBeenCalledWith(0.4)
  })

  /**
   * 进度轮询带回的会话音量回读：SetVolume fire-and-forget 丢失后，界面 30%、
   * 实际 100% 的失步必须在校对中自愈（重发一次），锁定标记同步给 UI。
   */
  it('reconciles a lost volume update from the polled session volume readback', async () => {
    const now = 1_000
    let progressTimer: (() => void) | undefined
    const setVolumeCalls: number[] = []

    // 会话实际生效 1.0（SetVolume 丢了），前端认为 30%
    const getPlaybackStatus = vi.fn(async () => ({
      progressMs: 0,
      isBuffering: false,
      volume: 1,
      volumeLocked: false
    }))

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
        get_playback_status: getPlaybackStatus,
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 100, isComplete: true })),
        set_volume: vi.fn(async (volume: number) => {
          setVolumeCalls.push(volume)
        })
      }
    } as unknown as Window & typeof globalThis)

    const configStore = useConfigStore()
    configStore.softwareVolume = true
    configStore.volumePercent = 30

    const playbackStore = usePlaybackStore()
    // isPlaying watch 异步 flush 后 startTimer 才注册定时器；等一拍再手动 tick。
    playbackStore.isPlaying = true
    await new Promise((resolve) => setTimeout(resolve, 10))
    progressTimer?.()
    // 校对重发走 80ms 尾沿节流，等节流窗口过去再断言
    await new Promise((resolve) => setTimeout(resolve, 150))

    expect(setVolumeCalls).toContain(0.3)
  })

  /**
   * 快速连点「播放全部」：第一次点击的请求还在途中（目标歌已乐观显示在 UI 上、
   * 尚未装载），第二次点击同一目标时**不得**把这次请求当成"已装载且已暂停"
   * 去 resume —— 那样续上的是上一首，界面切了歌、声音还是旧歌。
   *
   * 不变式：只有「引擎里装载的正是这首歌、且没有请求在途」才允许恢复播放。
   */
  it('plays the newly targeted song instead of resuming the old session on a second rapid playAll', async () => {
    const calls: string[] = []
    let releaseUrlFetch: (() => void) | undefined
    let urlFetchGate: Promise<void> | undefined

    const makeSong = (id: number): CurrentSong => ({
      id,
      name: `Song ${id}`,
      artists: [{ id: 1, name: 'Artist' }],
      cover: 'cover',
      duration: 180000
    })

    vi.stubGlobal(
      'setInterval',
      vi.fn(() => 1 as unknown as ReturnType<typeof setInterval>)
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
            ],
            privileges: [{ id: ids[0], playMaxBrLevel: 'standard', chargeInfoList: [] }]
          }
        })),
        song_url: vi.fn(async ({ id }: { id: number }) => {
          // 第一次点「播放全部」时把 URL 请求卡住，制造"请求在途、目标歌尚未装载"的窗口
          if (urlFetchGate) {
            const gate = urlFetchGate
            urlFetchGate = undefined
            await gate
          }
          return { body: { data: [{ url: `https://example.com/${id}.mp3`, level: 'standard' }] } }
        }),
        prepare_cached_song_source: vi.fn(async ({ songId }: { songId: number }) => ({
          type: 'file',
          value: `/tmp/${songId}.flac`
        })),
        get_output_devices: vi.fn(async () => [
          { id: 'default', name: 'System Default', isDefault: true, isCurrent: true }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => {
          calls.push('stop')
        }),
        play_file: vi.fn(async (path: string) => {
          calls.push(`play_file:${path}`)
        }),
        resume: vi.fn(async () => {
          calls.push('resume')
        }),
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()

    await playerStore.playMusic(1)
    expect(calls).toContain('play_file:/tmp/1.flac')

    urlFetchGate = new Promise<void>((resolve) => {
      releaseUrlFetch = resolve
    })
    const firstClick = playerStore.playAll([makeSong(2)])
    // UI 已经显示第二首（loading 期间列表要立即高亮），但引擎里仍是第一首
    for (let i = 0; i < 500 && playerStore.currentSongId !== 2; i += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
    expect(playerStore.currentSongId).toBe(2)

    await playerStore.playAll([makeSong(2)])
    releaseUrlFetch?.()
    await firstClick

    expect(calls).toContain('play_file:/tmp/2.flac')
    expect(calls).not.toContain('resume')
    expect(playerStore.currentSongId).toBe(2)
    expect(playerStore.isPlaying).toBe(true)
  })

  /**
   * 被取代的开播请求不得回写任何 UI 状态：它在 await 之后才发现自己过期，
   * 如果继续写 currentSong / isPlaying，界面就会停在那个已经没人要的歌上。
   */
  it('drops a superseded playback request instead of writing its song to the UI', async () => {
    let releaseDetail: (() => void) | undefined
    let detailGate: Promise<void> | undefined
    const played: string[] = []

    vi.stubGlobal(
      'setInterval',
      vi.fn(() => 1 as unknown as ReturnType<typeof setInterval>)
    )
    vi.stubGlobal('clearInterval', vi.fn())
    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: vi.fn(async ({ ids }: { ids: number[] }) => {
          // 第二首的详情请求被卡住，期间用户又点了第三首
          if (ids[0] === 2 && detailGate) {
            const gate = detailGate
            detailGate = undefined
            await gate
          }
          return {
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
              privileges: [{ id: ids[0], playMaxBrLevel: 'standard', chargeInfoList: [] }]
            }
          }
        }),
        song_url: vi.fn(async ({ id }: { id: number }) => ({
          body: { data: [{ url: `https://example.com/${id}.mp3`, level: 'standard' }] }
        })),
        prepare_cached_song_source: vi.fn(async ({ songId }: { songId: number }) => ({
          type: 'file',
          value: `/tmp/${songId}.flac`
        })),
        get_output_devices: vi.fn(async () => [
          { id: 'default', name: 'System Default', isDefault: true, isCurrent: true }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_file: vi.fn(async (path: string) => {
          played.push(path)
        }),
        resume: vi.fn(async () => undefined),
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()

    detailGate = new Promise<void>((resolve) => {
      releaseDetail = resolve
    })
    const superseded = playerStore.playMusic(2)
    await Promise.resolve()

    await playerStore.playMusic(3)
    expect(playerStore.currentSongId).toBe(3)

    releaseDetail?.()
    await superseded

    expect(playerStore.currentSongId).toBe(3)
    expect(playerStore.currentSong?.name).toBe('Song 3')
    expect(played).toEqual(['/tmp/3.flac'])
  })

  /**
   * 用户点停止时若正好有一次开播请求在途中，请求完成也不能把歌重新播起来。
   * 停止必须先作废会话，再 await 引擎停止。
   */
  it('does not let an in-flight start resurrect playback after stop', async () => {
    let releaseDetail: (() => void) | undefined
    let detailGate: Promise<void> | undefined
    const played: string[] = []

    vi.stubGlobal(
      'setInterval',
      vi.fn(() => 1 as unknown as ReturnType<typeof setInterval>)
    )
    vi.stubGlobal('clearInterval', vi.fn())
    vi.stubGlobal('window', {
      requestAnimationFrame: vi.fn(() => 1),
      cancelAnimationFrame: vi.fn(),
      api: {
        song_detail: vi.fn(async ({ ids }: { ids: number[] }) => {
          if (detailGate) {
            const gate = detailGate
            detailGate = undefined
            await gate
          }
          return {
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
              privileges: [{ id: ids[0], playMaxBrLevel: 'standard', chargeInfoList: [] }]
            }
          }
        }),
        song_url: vi.fn(async ({ id }: { id: number }) => ({
          body: { data: [{ url: `https://example.com/${id}.mp3`, level: 'standard' }] }
        })),
        prepare_cached_song_source: vi.fn(async ({ songId }: { songId: number }) => ({
          type: 'file',
          value: `/tmp/${songId}.flac`
        })),
        get_output_devices: vi.fn(async () => [
          { id: 'default', name: 'System Default', isDefault: true, isCurrent: true }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_file: vi.fn(async (path: string) => {
          played.push(path)
        }),
        resume: vi.fn(async () => undefined),
        get_progress: vi.fn(async () => 0),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()

    detailGate = new Promise<void>((resolve) => {
      releaseDetail = resolve
    })
    const pending = playerStore.playMusic(1)
    await Promise.resolve()

    await playerStore.stopMusic()
    releaseDetail?.()
    await pending

    expect(played).toEqual([])
    expect(playerStore.currentSongId).toBeNull()
    expect(playerStore.isPlaying).toBe(false)
  })

  it('still resumes a genuinely paused song without reloading it', async () => {
    const playFile = vi.fn(async () => undefined)
    const resume = vi.fn(async () => undefined)
    const pause = vi.fn(async () => undefined)

    vi.stubGlobal(
      'setInterval',
      vi.fn(() => 1 as unknown as ReturnType<typeof setInterval>)
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
            ],
            privileges: [{ id: ids[0], playMaxBrLevel: 'standard', chargeInfoList: [] }]
          }
        })),
        song_url: vi.fn(async ({ id }: { id: number }) => ({
          body: { data: [{ url: `https://example.com/${id}.mp3`, level: 'standard' }] }
        })),
        prepare_cached_song_source: vi.fn(async ({ songId }: { songId: number }) => ({
          type: 'file',
          value: `/tmp/${songId}.flac`
        })),
        get_output_devices: vi.fn(async () => [
          { id: 'default', name: 'System Default', isDefault: true, isCurrent: true }
        ]),
        switch_output_device: vi.fn(async () => undefined),
        stop: vi.fn(async () => undefined),
        play_file: playFile,
        pause,
        resume,
        get_progress: vi.fn(async () => 5_000),
        is_buffering: vi.fn(async () => false),
        get_cached_song_progress: vi.fn(async () => ({ percent: 0 })),
        wait_finished: vi.fn(() => new Promise(() => undefined))
      }
    } as unknown as Window & typeof globalThis)

    const playerStore = usePlayerStore()

    await playerStore.playMusic(1)
    expect(playFile).toHaveBeenCalledTimes(1)

    await playerStore.togglePlay()
    expect(pause).toHaveBeenCalledTimes(1)
    expect(playerStore.isPlaying).toBe(false)

    await playerStore.playMusic(1)

    // 同一会话恢复：不重新装载，只 resume
    expect(playFile).toHaveBeenCalledTimes(1)
    expect(resume).toHaveBeenCalledTimes(1)
    expect(playerStore.isPlaying).toBe(true)
  })
})
