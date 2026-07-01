import { createPinia, setActivePinia } from 'pinia'
import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest'
import { useConfigStore } from '../configStore'
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
})
