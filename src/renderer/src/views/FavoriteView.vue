<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import MediaDetailLayout from '@renderer/components/media/MediaDetailLayout.vue'
import SongList from '@renderer/components/media/SongList.vue'
import { useFavoriteStore } from '@renderer/stores/favoriteStore'
import { CurrentSong, usePlayerStore } from '@renderer/stores/playerStore'
import type { Song } from '@renderer/types/songDetail'

const favoriteStore = useFavoriteStore()
const playerStore = usePlayerStore()
const searchQuery = ref('')

const mapFavoriteToSong = (song: CurrentSong): Song => ({
  id: song.id,
  name: song.name,
  dt: song.duration,
  ar: song.artists,
  al: {
    id: 0,
    name: '喜欢的音乐',
    picUrl: song.cover
  }
})

const songs = computed(() => favoriteStore.favoriteSongs.map(mapFavoriteToSong))

const filteredSongs = computed(() => {
  const query = searchQuery.value.trim().toLowerCase()
  if (!query) return songs.value

  return songs.value.filter((song) => {
    return (
      song.name.toLowerCase().includes(query) ||
      song.ar.some((artist) => artist.name.toLowerCase().includes(query))
    )
  })
})

const coverUrl = computed(() => favoriteStore.favoriteSongs[0]?.cover || '')

const handlePlayAll = (): void => {
  if (!favoriteStore.favoriteSongs.length) return
  void playerStore.playAll(favoriteStore.favoriteSongs)
}

const handlePlaySong = (song: Song): void => {
  void playerStore.playMusic(song.id)
}

onMounted(() => {
  void favoriteStore.fetchFavoriteSongs(true)
})
</script>

<template>
  <MediaDetailLayout
    v-model:search-query="searchQuery"
    :loading="favoriteStore.isLoading"
    :cover-url="coverUrl"
    title="喜欢的音乐"
    :description="favoriteStore.errorMessage || '通过网易云账号同步的喜欢歌曲。'"
    :meta="[`${favoriteStore.favoriteCount} 首歌曲`]"
    search-placeholder="在喜欢的音乐中搜索..."
    @play-all="handlePlayAll"
  >
    <SongList :songs="filteredSongs" :search-query="searchQuery" @play="handlePlaySong" />
  </MediaDetailLayout>
</template>
