import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { CurrentSong, PlayMode } from '@renderer/types/player'
import { usePlaylistStore } from '../playlist'

const song = (id: number): CurrentSong => ({
  id,
  name: `Song ${id}`,
  artists: [{ id: 1, name: 'Artist' }],
  cover: 'cover',
  duration: 180000
})

describe('play next queue', () => {
  const storage = new Map<string, string>()

  beforeEach(() => {
    storage.clear()
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key)
    })
    setActivePinia(createPinia())
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('inserts a song immediately after the current song and persists the queue', async () => {
    const store = usePlaylistStore()
    store.setPlaylist([song(1), song(2), song(3)])
    store.currentSongId = 2

    expect(store.addNext(song(4))).toBe(true)
    expect(store.playlist.map((item) => item.id)).toEqual([1, 2, 4, 3])
    expect(store.currentSongId).toBe(2)
    expect(store.getNextSongId()).toBe(4)
    expect(store.playlist[2]).toEqual(song(4))
    await nextTick()
    expect(JSON.parse(storage.get('playlist')!)).toEqual(store.playlist)
  })

  it.each([1, 4])('moves an existing song from either side without duplicating it: %i', (id) => {
    const store = usePlaylistStore()
    store.setPlaylist([song(1), song(2), song(3), song(4)])
    store.currentSongId = 2
    store.addNext(song(id))
    store.addNext(song(id))

    expect(store.playlist).toHaveLength(4)
    expect(store.playlist[store.currentIndex + 1].id).toBe(id)
    expect(store.getNextSongId()).toBe(id)
  })

  it.each([null, 99])(
    'handles an empty queue or a current song absent from the queue: %s',
    (id) => {
      const store = usePlaylistStore()
      store.currentSongId = id
      store.addNext(song(4))
      expect(store.playlist.map((item) => item.id)).toEqual([4])
      expect(store.getNextSongId()).toBe(4)
    }
  )

  it('does not move or duplicate the currently playing song', () => {
    const store = usePlaylistStore()
    store.setPlaylist([song(1), song(2), song(3)])
    store.currentSongId = 2
    expect(store.addNext(song(2))).toBe(false)
    expect(store.playlist.map((item) => item.id)).toEqual([1, 2, 3])
  })

  it.each<PlayMode>(['loop', 'random', 'single'])(
    'inserts songs in queue order and keeps the existing %s mode',
    (mode) => {
      const store = usePlaylistStore()
      store.setPlaylist([song(1), song(2)])
      store.currentSongId = 1
      store.playMode = mode
      store.addNext(song(3))
      store.addNext(song(4))
      expect(store.playlist.map((item) => item.id)).toEqual([1, 4, 3, 2])
      vi.spyOn(Math, 'random').mockReturnValue(0)
      expect(store.getNextSongId(true)).toBe(mode === 'loop' ? 4 : 1)
      expect(store.getNextSongId()).toBe(mode === 'random' ? 1 : 4)

      store.currentSongId = 4
      expect(store.getNextSongId(true)).toBe(mode === 'single' ? 4 : mode === 'random' ? 1 : 3)
      store.currentSongId = 3
      expect(store.getNextSongId(true)).toBe(mode === 'single' ? 3 : mode === 'random' ? 1 : 2)
      expect(store.playMode).toBe(mode)
    }
  )

  it('uses the remaining order after removing, replacing or clearing songs', () => {
    const store = usePlaylistStore()
    store.setPlaylist([song(1), song(2)])
    store.currentSongId = 1
    store.addNext(song(3))
    store.addNext(song(4))
    expect(store.removeFromPlaylist(4)).toBe(true)
    expect(store.getNextSongId()).toBe(3)

    store.setPlaylist([song(1), song(2), song(3)])
    expect(store.getNextSongId()).toBe(2)
    store.addNext(song(3))
    store.clearPlaylist()
    expect(store.getNextSongId()).toBeNull()
    store.addToPlaylist(song(2))
    store.addToPlaylist(song(3))
    expect(store.getNextSongId()).toBe(2)
  })

  it('deduplicates replacements and removes or updates songs through store methods', async () => {
    const store = usePlaylistStore()
    store.setPlaylist([song(1), song(2), song(1), song(3)])
    store.selectSong(2)
    store.updateSong({ ...song(2), duration: 240000 })
    expect(store.playlist.map((item) => item.id)).toEqual([1, 2, 3])
    expect(store.playlist[1].duration).toBe(240000)
    expect(store.removeSongs([1, 3, 99])).toBe(2)
    expect(store.removeFromPlaylist(99)).toBe(false)
    expect(store.currentSongId).toBe(2)
    await nextTick()
    expect(JSON.parse(storage.get('playlist')!)).toEqual([store.playlist[0]])
    expect(storage.get('currentSongId')).toBe('2')
    setActivePinia(createPinia())
    const restored = usePlaylistStore()
    expect(restored.playlist).toEqual(store.playlist)
    expect(restored.currentSongId).toBe(2)
    restored.selectSong(null)
    await nextTick()
    expect(storage.has('currentSongId')).toBe(false)
  })
})
