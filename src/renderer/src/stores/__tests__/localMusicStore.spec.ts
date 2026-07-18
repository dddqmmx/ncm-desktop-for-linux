import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useLocalMusicStore } from '../localMusicStore'

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

class FakeAudio {
  preload = ''
  src = ''
  duration = 125.4
  private readonly listeners = new Map<string, Array<() => void>>()

  addEventListener(name: string, callback: () => void): void {
    const callbacks = this.listeners.get(name) ?? []
    callbacks.push(callback)
    this.listeners.set(name, callbacks)
  }

  removeAttribute(): void {
    this.src = ''
  }

  load(): void {
    if (this.src) this.listeners.get('loadedmetadata')?.forEach((callback) => callback())
  }
}

describe('localMusicStore', () => {
  let storage: MemoryStorage
  const getPathForFile = vi.fn((file: File) => `/music/${file.name}`)

  beforeEach(() => {
    storage = new MemoryStorage()
    setActivePinia(createPinia())
    vi.stubGlobal('localStorage', storage)
    vi.stubGlobal('Audio', FakeAudio)
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => 'blob:test-audio'),
      revokeObjectURL: vi.fn()
    })
    vi.stubGlobal('window', {
      api: { get_path_for_file: getPathForFile },
      setTimeout,
      clearTimeout
    } as unknown as Window & typeof globalThis)
  })

  afterEach(() => {
    getPathForFile.mockClear()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('imports audio files, derives display metadata and persists the library', async () => {
    const store = useLocalMusicStore()
    const file = { name: 'Artist - Track.mp3', type: 'audio/mpeg' } as File

    const result = await store.importFiles([file])
    await nextTick()

    expect(result).toEqual({ imported: 1, skipped: 0, failed: [] })
    expect(store.songs).toHaveLength(1)
    expect(store.songs[0]).toMatchObject({
      name: 'Track',
      artists: [{ id: 0, name: 'Artist' }],
      duration: 125_400,
      source: 'local',
      filePath: '/music/Artist - Track.mp3',
      fileName: 'Artist - Track.mp3'
    })
    expect(store.songs[0].id).toBeLessThan(0)
    expect(JSON.parse(storage.getItem('localMusicLibrary') ?? '[]')).toHaveLength(1)
  })

  it('skips duplicate paths and unsupported files', async () => {
    const store = useLocalMusicStore()
    const song = { name: 'Track.flac', type: 'audio/flac' } as File
    const text = { name: 'notes.txt', type: 'text/plain' } as File

    await store.importFiles([song])
    const result = await store.importFiles([song, text])

    expect(result).toEqual({ imported: 0, skipped: 2, failed: [] })
    expect(store.songs).toHaveLength(1)
  })
})
