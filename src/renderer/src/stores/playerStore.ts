import { defineStore } from 'pinia'
import { computed } from 'vue'
import { usePlaylistStore } from './player/playlist'
import { usePlaybackStore } from './player/playback'
import { usePlayerUiStore } from './player/ui'

export * from '@renderer/types/player'
export * from './player/utils'

export const usePlayerStore = defineStore('player', () => {
  const playlistStore = usePlaylistStore()
  const playbackStore = usePlaybackStore()
  const uiStore = usePlayerUiStore()

  return {
    // Playback State
    currentSong: computed({
      get: () => playbackStore.currentSong,
      set: (val) => (playbackStore.currentSong = val)
    }),
    currentSongId: computed({
      get: () => playbackStore.currentSongId,
      set: (val) => (playbackStore.currentSongId = val)
    }),
    currentTime: computed({
      get: () => playbackStore.currentTime,
      set: (val) => (playbackStore.currentTime = val)
    }),
    isPlaying: computed({
      get: () => playbackStore.isPlaying,
      set: (val) => (playbackStore.isPlaying = val)
    }),
    isSeeking: computed({
      get: () => playbackStore.isSeeking,
      set: (val) => (playbackStore.isSeeking = val)
    }),
    duration: computed(() => playbackStore.duration),
    progressPercent: computed(() => playbackStore.progressPercent),

    // Playlist State
    playlist: computed({
      get: () => playlistStore.playlist,
      set: (val) => (playlistStore.playlist = val)
    }),
    playMode: computed({
      get: () => playlistStore.playMode,
      set: (val) => (playlistStore.playMode = val)
    }),
    currentIndex: computed(() => playlistStore.currentIndex),

    // UI State
    isFullScreen: computed({
      get: () => uiStore.isFullScreen,
      set: (val) => (uiStore.isFullScreen = val)
    }),

    // Actions
    initFromStorage: playbackStore.initFromStorage,
    playAll: playbackStore.playAll,
    playMusic: playbackStore.playMusic,
    togglePlay: playbackStore.togglePlay,
    seek: playbackStore.seek,
    toggleFullScreen: uiStore.toggleFullScreen,
    playNext: playbackStore.playNext,
    playPrev: playbackStore.playPrev,
    setPlaylist: playlistStore.setPlaylist,
    clearPlaylist: playlistStore.clearPlaylist,
    addToPlaylist: playlistStore.addToPlaylist,
    togglePlayMode: playlistStore.togglePlayMode
  }
})
