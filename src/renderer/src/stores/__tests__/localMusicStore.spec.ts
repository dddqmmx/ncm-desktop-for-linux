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
  const checkLibraryFiles = vi.fn(async () => ({ missingPaths: [] as string[] }))
  const getLibraryFileDurations = vi.fn(
    async () => [] as Array<{ filePath: string; duration: number }>
  )

  beforeEach(() => {
    storage = new MemoryStorage()
    setActivePinia(createPinia())
    vi.stubGlobal('localStorage', storage)
    vi.stubGlobal('Audio', FakeAudio)
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => 'blob:test-audio'),
      revokeObjectURL: vi.fn()
    })
    checkLibraryFiles.mockImplementation(async () => ({ missingPaths: [] }))
    getLibraryFileDurations.mockImplementation(async () => [])
    vi.stubGlobal('window', {
      api: {
        get_path_for_file: getPathForFile,
        check_library_files: checkLibraryFiles,
        get_library_file_durations: getLibraryFileDurations
      },
      setTimeout,
      clearTimeout
    } as unknown as Window & typeof globalThis)
  })

  afterEach(() => {
    getPathForFile.mockClear()
    checkLibraryFiles.mockReset()
    getLibraryFileDurations.mockReset()
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

  it('imports scanned file paths and removes songs under a library root', async () => {
    const store = useLocalMusicStore()

    const result = await store.importPaths([
      { filePath: '/music/lib/Artist - One.mp3', fileName: 'Artist - One.mp3' },
      { filePath: '/music/lib/Artist - One.mp3', fileName: 'Artist - One.mp3' },
      { filePath: '/music/other/Two.flac', fileName: 'Two.flac', duration: 95_000 },
      { filePath: '/music/lib/readme.txt', fileName: 'readme.txt' }
    ])

    expect(result).toEqual({ imported: 2, skipped: 2, failed: [] })
    expect(store.songs).toHaveLength(2)
    expect(store.removeSongsUnderPath('/music/lib')).toBe(1)
    expect(store.songs).toHaveLength(1)
    expect(store.songs[0].filePath).toBe('/music/other/Two.flac')
    expect(store.songs[0].duration).toBe(95_000)
  })

  it('fills missing durations in an existing persisted library', async () => {
    storage.setItem(
      'localMusicLibrary',
      JSON.stringify([
        {
          id: -1,
          name: 'Unknown Duration',
          artists: [{ id: 0, name: 'Artist' }],
          duration: 0,
          source: 'local',
          filePath: '/music/unknown.mp3',
          fileName: 'unknown.mp3'
        }
      ])
    )
    getLibraryFileDurations.mockResolvedValue([
      { filePath: '/music/unknown.mp3', duration: 183_250 }
    ])

    const store = useLocalMusicStore()
    await vi.waitFor(() => expect(store.songs[0].duration).toBe(183_250))
    await nextTick()

    expect(getLibraryFileDurations).toHaveBeenCalledWith(['/music/unknown.mp3'])
    expect(JSON.parse(storage.getItem('localMusicLibrary') ?? '[]')[0].duration).toBe(183_250)
  })

  it('removes missing files whenever the persisted library is loaded', async () => {
    storage.setItem(
      'localMusicLibrary',
      JSON.stringify([
        {
          id: -1,
          name: 'Existing',
          artists: [{ id: 0, name: 'Artist' }],
          duration: 1_000,
          source: 'local',
          filePath: '/music/existing.mp3',
          fileName: 'existing.mp3'
        },
        {
          id: -2,
          name: 'Missing',
          artists: [{ id: 0, name: 'Artist' }],
          duration: 1_000,
          source: 'local',
          filePath: '/music/missing.mp3',
          fileName: 'missing.mp3'
        }
      ])
    )
    checkLibraryFiles.mockResolvedValue({ missingPaths: ['/music/missing.mp3'] })

    const store = useLocalMusicStore()
    await vi.waitFor(() => expect(store.songs).toHaveLength(1))
    await nextTick()

    expect(checkLibraryFiles).toHaveBeenCalledWith(['/music/existing.mp3', '/music/missing.mp3'])
    expect(store.songs[0].filePath).toBe('/music/existing.mp3')
    expect(JSON.parse(storage.getItem('localMusicLibrary') ?? '[]')).toHaveLength(1)
  })
})
