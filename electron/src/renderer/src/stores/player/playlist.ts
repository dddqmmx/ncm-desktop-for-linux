import { defineStore } from 'pinia'
import { ref, watch, computed } from 'vue'
import { CurrentSong, PlayMode } from '@renderer/types/player'
import { loadPersistedPlaylist, normalizeCurrentSong } from './utils'

export const usePlaylistStore = defineStore('playlist', () => {
  const playlist = ref<CurrentSong[]>(loadPersistedPlaylist())
  const playMode = ref<PlayMode>((localStorage.getItem('playMode') as PlayMode) || 'loop')
  const currentSongId = ref<number | null>(Number(localStorage.getItem('currentSongId')) || null)

  const currentIndex = computed(() => {
    return playlist.value.findIndex((s) => s.id === currentSongId.value)
  })

  const setPlaylist = (list: CurrentSong[]): void => {
    const seen = new Set<number>()
    playlist.value = list
      .map((song) => normalizeCurrentSong(song))
      .filter((song): song is CurrentSong => song !== null)
      .filter((song) => {
        if (seen.has(song.id)) return false
        seen.add(song.id)
        return true
      })
  }

  const clearPlaylist = (): void => {
    playlist.value = []
  }

  const removeSongs = (ids: readonly number[]): number => {
    const removedIds = new Set(ids)
    const previousCount = playlist.value.length
    playlist.value = playlist.value.filter((song) => !removedIds.has(song.id))
    return previousCount - playlist.value.length
  }

  const removeFromPlaylist = (id: number): boolean => removeSongs([id]) > 0

  const updateSong = (song: CurrentSong): void => {
    const normalizedSong = normalizeCurrentSong(song)
    if (!normalizedSong) return
    playlist.value = playlist.value.map((item) =>
      item.id === normalizedSong.id ? normalizedSong : item
    )
  }

  const selectSong = (id: number | null): void => {
    currentSongId.value = id
  }

  const addToPlaylist = (song: CurrentSong): void => {
    const normalizedSong = normalizeCurrentSong(song)
    if (!normalizedSong) return

    if (!playlist.value.some((s) => s.id === normalizedSong.id)) {
      playlist.value.push(normalizedSong)
    }
  }

  const addNext = (song: CurrentSong): boolean => {
    const normalizedSong = normalizeCurrentSong(song)
    if (!normalizedSong || normalizedSong.id === currentSongId.value) return false

    const remaining = playlist.value.filter((item) => item.id !== normalizedSong.id)
    const playingIndex = remaining.findIndex((item) => item.id === currentSongId.value)
    remaining.splice(playingIndex + 1, 0, normalizedSong)
    playlist.value = remaining
    return true
  }

  const togglePlayMode = (): void => {
    const modes: PlayMode[] = ['loop', 'random', 'single']
    const nextIdx = (modes.indexOf(playMode.value) + 1) % modes.length
    playMode.value = modes[nextIdx]
  }

  const getNextSongId = (isAuto = false): number | null => {
    if (playlist.value.length === 0) return null

    if (isAuto && playMode.value === 'single' && currentIndex.value >= 0) {
      return currentSongId.value
    }

    let nextIndex = 0
    if (playMode.value === 'random') {
      nextIndex = Math.floor(Math.random() * playlist.value.length)
    } else {
      nextIndex = currentIndex.value + 1
      if (nextIndex >= playlist.value.length) {
        nextIndex = 0
      }
    }

    return playlist.value[nextIndex]?.id ?? null
  }

  const getPrevSongId = (): number | null => {
    if (playlist.value.length === 0) return null

    let prevIndex = 0
    if (playMode.value === 'random') {
      prevIndex = Math.floor(Math.random() * playlist.value.length)
    } else {
      prevIndex = currentIndex.value - 1
      if (prevIndex < 0) {
        prevIndex = playlist.value.length - 1
      }
    }

    return playlist.value[prevIndex]?.id ?? null
  }

  watch(
    playlist,
    (newList) => {
      localStorage.setItem('playlist', JSON.stringify(newList))
    },
    { deep: true }
  )

  watch(playMode, (newMode) => {
    localStorage.setItem('playMode', newMode)
  })

  watch(currentSongId, (id) => {
    if (id === null) localStorage.removeItem('currentSongId')
    else localStorage.setItem('currentSongId', id.toString())
  })

  return {
    playlist,
    playMode,
    currentSongId,
    currentIndex,
    setPlaylist,
    clearPlaylist,
    removeFromPlaylist,
    removeSongs,
    updateSong,
    selectSong,
    addToPlaylist,
    addNext,
    togglePlayMode,
    getNextSongId,
    getPrevSongId
  }
})
