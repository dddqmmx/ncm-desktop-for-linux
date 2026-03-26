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
    playlist.value = list
      .map((song) => normalizeCurrentSong(song))
      .filter((song): song is CurrentSong => song !== null)
  }

  const clearPlaylist = (): void => {
    playlist.value = []
  }

  const addToPlaylist = (song: CurrentSong): void => {
    const normalizedSong = normalizeCurrentSong(song)
    if (!normalizedSong) return

    if (!playlist.value.some((s) => s.id === normalizedSong.id)) {
      playlist.value.push(normalizedSong)
    }
  }

  const togglePlayMode = (): void => {
    const modes: PlayMode[] = ['loop', 'random', 'single']
    const nextIdx = (modes.indexOf(playMode.value) + 1) % modes.length
    playMode.value = modes[nextIdx]
  }

  const getNextSongId = (isAuto = false): number | null => {
    if (playlist.value.length === 0) return null

    if (isAuto && playMode.value === 'single' && currentSongId.value) {
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

  return {
    playlist,
    playMode,
    currentSongId,
    currentIndex,
    setPlaylist,
    clearPlaylist,
    addToPlaylist,
    togglePlayMode,
    getNextSongId,
    getPrevSongId
  }
})
