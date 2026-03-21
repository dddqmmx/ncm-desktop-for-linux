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
      artist: 'Artist',
      cover: 'cover',
      duration: 180000
    }
    playerStore.isPlaying = true

    await playerStore.playMusic(2)

    expect(callOrder).toEqual(['stop', 'switch:headphones', 'play_url'])
    expect(configStore.outputDeviceId).toBe('headphones')
    expect(currentDeviceId).toBe('headphones')
  })
})
