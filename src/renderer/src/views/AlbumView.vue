<script setup lang="ts">
import { useRoute } from 'vue-router'
import { ref, computed, watch } from 'vue'
import { CurrentSong, createCurrentSongArtists, usePlayerStore } from '@renderer/stores/playerStore'
import { AlbumDetail, AlbumDetailInfo } from '@renderer/types/album'
import { Song } from '@renderer/types/songDetail'
import MediaDetailLayout from '../components/MediaDetailLayout.vue'
import SongList from '../components/SongList.vue'

const route = useRoute()
const playerStore = usePlayerStore()

// --- 响应式数据 ---
const album = ref<AlbumDetailInfo | null>(null)
const tracks = ref<Song[]>([])
const loading = ref(true)
const searchQuery = ref('')

// --- 格式化工具 ---
const formatDate = (timestamp: number): string => {
  const date = new Date(timestamp)
  return `${date.getFullYear()}-${(date.getMonth() + 1).toString().padStart(2, '0')}-${date.getDate().toString().padStart(2, '0')}`
}

// --- 数据获取 ---
const fetchAlbumDetail = async (albumId: string | string[]): Promise<void> => {
  try {
    loading.value = true
    const res = (await window.api.album({ id: albumId })) as { body?: AlbumDetail }
    if (res.body && res.body.album) {
      album.value = res.body.album
      tracks.value = res.body.songs || []
    }
  } catch (error) {
    console.error('Failed to fetch album detail:', error)
  } finally {
    loading.value = false
  }
}

// --- 辅助方法：将 Track 转换为 CurrentSong ---
const mapTrackToCurrentSong = (track: Song): CurrentSong => ({
  id: track.id,
  name: track.name,
  artists: createCurrentSongArtists(track.ar),
  cover: album.value?.picUrl || track.al?.picUrl || '',
  duration: track.dt
})

// --- 处理“播放全部”按钮点击 ---
const handlePlayAll = (): void => {
  if (!tracks.value.length) return
  const songList = tracks.value.map(mapTrackToCurrentSong)
  void playerStore.playAll(songList)
}

const handlePlaySong = (song: Song): void => {
  void playerStore.playMusic(song.id)
}

watch(
  () => route.params.id,
  (albumId) => {
    if (!albumId) return
    fetchAlbumDetail(albumId)
  },
  { immediate: true }
)

const filteredTracks = computed(() => {
  if (!searchQuery.value.trim()) {
    return tracks.value
  }
  const query = searchQuery.value.toLowerCase()
  return tracks.value.filter((track) => {
    return (
      track.name.toLowerCase().includes(query) ||
      (track.al?.name || '').toLowerCase().includes(query) ||
      track.ar.some((artist) => artist.name.toLowerCase().includes(query))
    )
  })
})
</script>

<template>
  <MediaDetailLayout
    v-model:search-query="searchQuery"
    :loading="loading"
    :cover-url="album?.picUrl"
    :title="album?.name"
    :description="album?.description"
    :meta="
      album
        ? ([`${formatDate(album.publishTime)} 发行`, `${album.size} 首歌曲`, album.company].filter(
            Boolean
          ) as string[])
        : []
    "
    search-placeholder="在专辑内搜索..."
    @play-all="handlePlayAll"
  >
    <template #creator>
      <span v-if="album" class="creator-name">
        {{ album.artists.map((a) => a.name).join(' / ') }}
      </span>
    </template>

    <SongList
      :songs="filteredTracks"
      :search-query="searchQuery"
      :fallback-cover="album?.picUrl"
      @play="handlePlaySong"
    />
  </MediaDetailLayout>
</template>

<style scoped>
.creator-name {
  font-weight: 700;
  color: #333;
}

.create-time {
  color: #888;
  font-weight: 500;
  margin-left: 10px;
}
</style>
