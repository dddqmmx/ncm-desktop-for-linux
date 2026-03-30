<script setup lang="ts">
import { useRoute } from 'vue-router'
import { ref, computed, watch } from 'vue'
import { PlaylistDetail, Track } from '@renderer/types/playlistDetail'
import { CurrentSong, createCurrentSongArtists, usePlayerStore } from '@renderer/stores/playerStore'
import MediaDetailLayout from '../components/MediaDetailLayout.vue'
import SongList from '../components/SongList.vue'
import UserAvatar from '../components/UserAvatar.vue'

const route = useRoute()
const playerStore = usePlayerStore()

// --- 响应式数据 ---
const detail = ref<PlaylistDetail | null>(null)
const loading = ref(true)
const searchQuery = ref('')

// --- 格式化工具 ---
const formatCount = (num: number): string => {
  if (num >= 100000000) return (num / 100000000).toFixed(1) + '亿'
  if (num >= 10000) return (num / 10000).toFixed(1) + '万'
  return num.toString()
}

const formatDate = (timestamp: number): string => {
  const date = new Date(timestamp)
  return `${date.getFullYear()}-${(date.getMonth() + 1).toString().padStart(2, '0')}-${date.getDate().toString().padStart(2, '0')}`
}

// --- 数据获取 ---
const fetchPlaylistDetail = async (playlistId: string | string[]): Promise<void> => {
  try {
    loading.value = true
    const res = (await window.api.playlist_detail({ id: playlistId })) as { body?: PlaylistDetail }
    if (res.body) {
      detail.value = res.body
    }
  } catch (error) {
    console.error('Failed to fetch playlist detail:', error)
  } finally {
    loading.value = false
  }
}

// --- 辅助方法：将 Track 转换为 CurrentSong ---
const mapTrackToCurrentSong = (track: Track): CurrentSong => ({
  id: track.id,
  name: track.name,
  artists: createCurrentSongArtists(track.ar),
  cover: track.al.picUrl,
  duration: track.dt
})

// --- 处理“播放全部”按钮点击 ---
const handlePlayAll = (): void => {
  if (!tracks.value.length) return
  const songList = tracks.value.map(mapTrackToCurrentSong)
  void playerStore.playAll(songList)
}

const handlePlaySong = (song: Track): void => {
  void playerStore.playMusic(song.id)
}

watch(
  () => route.params.id,
  (playlistId) => {
    if (!playlistId) return
    fetchPlaylistDetail(playlistId)
  },
  { immediate: true }
)

const playlist = computed(() => detail.value?.playlist)
const tracks = computed(() => detail.value?.playlist.tracks || [])

const filteredTracks = computed(() => {
  if (!searchQuery.value.trim()) {
    return tracks.value
  }
  const query = searchQuery.value.toLowerCase()
  return tracks.value.filter((track) => {
    return (
      track.name.toLowerCase().includes(query) ||
      track.al.name.toLowerCase().includes(query) ||
      track.ar.some((artist) => artist.name.toLowerCase().includes(query))
    )
  })
})
</script>

<template>
  <MediaDetailLayout
    v-model:search-query="searchQuery"
    :loading="loading"
    :cover-url="playlist?.coverImgUrl"
    :title="playlist?.name"
    :description="playlist?.description"
    :meta="
      playlist
        ? [
            `${formatDate(playlist.createTime)} 创建`,
            `${playlist.trackCount} 首歌曲`,
            `${formatCount(playlist.playCount)} 次播放`
          ]
        : []
    "
    search-placeholder="在歌单内搜索..."
    @play-all="handlePlayAll"
  >
    <template #creator>
      <div v-if="playlist" class="creator-info">
        <div class="creator-avatar-wrapper">
          <UserAvatar :id="playlist.creator.avatarUrl" size="50y50" />
        </div>
        <span class="creator-name">{{ playlist.creator.nickname }}</span>
      </div>
    </template>

    <template #actions>
      <!-- 将创建时间放到 meta 里，或者通过 slot 调整 -->
      <button v-if="playlist" class="secondary-btn">
        <svg
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path
            d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.29 1.5 4.04 3 5.5l7 7 7-7z"
          ></path>
        </svg>
        {{ formatCount(playlist.subscribedCount) }}
      </button>
    </template>

    <SongList
      :songs="filteredTracks"
      :search-query="searchQuery"
      :fallback-cover="playlist?.coverImgUrl"
      @play="handlePlaySong"
    />
  </MediaDetailLayout>
</template>

<style scoped>
.creator-info {
  display: flex;
  align-items: center;
  gap: 8px;
}
.creator-avatar-wrapper {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  overflow: hidden;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.1);
}

.creator-name {
  font-weight: 700;
  color: #333;
}

.create-time {
  color: #888;
  font-weight: 500;
}

.secondary-btn {
  background: rgba(0, 0, 0, 0.04);
  border: 1px solid rgba(0, 0, 0, 0.05);
  padding: 0 16px;
  height: 38px;
  border-radius: 100px;
  display: flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
}
</style>
