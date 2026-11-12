<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { useRoute, useRouter } from 'vue-router'
import { ref, computed, watch } from 'vue'
import { PlaylistDetail, Track } from '@renderer/types/playlistDetail'
import { Song } from '@renderer/types/songDetail'
import { CurrentSong, createCurrentSongArtists, usePlayerStore } from '@renderer/stores/playerStore'
import { useUserStore } from '@renderer/stores/userStore'
import { useDialogStore } from '@renderer/stores/dialogStore'
import { usePlaylistStore, type PlaylistMetadata } from '@renderer/stores/playlistStore'
import { apiErrorMessage, isApiSuccess, type ServiceResult } from '@renderer/utils/api'
import MediaDetailLayout from '@renderer/components/media/MediaDetailLayout.vue'
import SongList from '@renderer/components/media/SongList.vue'
import UserAvatar from '@renderer/components/media/UserAvatar.vue'
import EditPlaylistModal from '@renderer/components/overlays/EditPlaylistModal.vue'
import PlaylistActions from '@renderer/components/media/PlaylistActions.vue'

const route = useRoute()
const router = useRouter()
const playerStore = usePlayerStore()
const userStore = useUserStore()
const playlistStore = usePlaylistStore()
const dialogStore = useDialogStore()

// --- 响应式数据 ---
const detail = ref<PlaylistDetail | null>(null)
const loading = ref(true)
const searchQuery = ref('')
const showEditPlaylist = ref(false)
const deleting = ref(false)
const errorMessage = ref('')
let detailRequestId = 0

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
  const requestId = ++detailRequestId
  detail.value = null
  errorMessage.value = ''
  try {
    loading.value = true
    const res = (await window.api.playlist_detail({
      id: playlistId,
      cookie: userStore.cookie
    })) as ServiceResult<PlaylistDetail>
    if (requestId !== detailRequestId) return
    if (!isApiSuccess(res) || !res.body?.playlist) {
      throw new Error(apiErrorMessage(res, '获取歌单详情失败'))
    }
    detail.value = res.body
  } catch (error) {
    if (requestId === detailRequestId) {
      errorMessage.value = error instanceof Error ? error.message : '获取歌单详情失败'
    }
  } finally {
    if (requestId === detailRequestId) loading.value = false
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

const handlePlaySong = (song: Song): void => {
  void playerStore.playMusic(song.id)
}

const handleTrackRemoved = (songId: number): void => {
  if (!detail.value) return
  detail.value.playlist.tracks = detail.value.playlist.tracks.filter((t) => t.id !== songId)
  detail.value.playlist.trackCount -= 1
}

const handlePlaylistSaved = (playlistId: number, metadata: PlaylistMetadata): void => {
  if (detail.value?.playlist.id !== playlistId) return
  Object.assign(detail.value.playlist, metadata)
  showEditPlaylist.value = false
}

const handleDeletePlaylist = async (): Promise<void> => {
  const target = playlist.value
  if (!target || !canManagePlaylist.value || deleting.value) return
  deleting.value = true
  errorMessage.value = ''

  try {
    const confirmed = await dialogStore.open({
      title: '删除歌单',
      message: `确定删除「${target.name}」吗？删除后无法恢复。`,
      mode: 'confirm-cancel',
      confirmText: '删除',
      cancelText: '取消'
    })
    if (!confirmed || playlist.value?.id !== target.id) return

    await playlistStore.deletePlaylist(target)
    if (route.name === 'playlist' && String(route.params.id) === String(target.id)) {
      await router.replace('/home')
    }
  } catch (error) {
    if (playlist.value?.id === target.id) {
      errorMessage.value = error instanceof Error ? error.message : '删除歌单失败，请重试'
    }
  } finally {
    deleting.value = false
  }
}

watch(
  () => route.params.id,
  (playlistId) => {
    showEditPlaylist.value = false
    searchQuery.value = ''
    if (!playlistId) return
    void fetchPlaylistDetail(playlistId)
  },
  { immediate: true }
)

const playlist = computed(() => detail.value?.playlist)
const tracks = computed(() => detail.value?.playlist.tracks || [])
const isOwnPlaylist = computed(
  () => !!playlist.value && playlist.value.creator.userId === userStore.userInfo?.account.id
)
const ownPlaylistId = computed(() => (isOwnPlaylist.value ? playlist.value?.id : undefined))
const canManagePlaylist = computed(
  () => !!playlist.value && playlistStore.canManagePlaylist(playlist.value)
)

watch(canManagePlaylist, (canManage) => {
  if (!canManage) showEditPlaylist.value = false
})

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
    class="playlist-view"
    :loading="loading"
    :show-more="false"
    :play-disabled="!tracks.length"
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
      <span v-if="playlist" class="subscriber-count" title="收藏人数">
        <AppIcon name="heart" :size="18" />
        {{ formatCount(playlist.subscribedCount) }}
      </span>
      <PlaylistActions
        v-if="canManagePlaylist"
        :disabled="deleting"
        @edit="showEditPlaylist = true"
        @delete="handleDeletePlaylist"
      />
    </template>

    <p v-if="errorMessage" class="error-message" role="alert">{{ errorMessage }}</p>
    <SongList
      :songs="filteredTracks"
      :search-query="searchQuery"
      :fallback-cover="playlist?.coverImgUrl"
      :playlist-id="ownPlaylistId"
      @play="handlePlaySong"
      @removed="handleTrackRemoved"
    />
  </MediaDetailLayout>
  <EditPlaylistModal
    v-if="showEditPlaylist && playlist && canManagePlaylist"
    :key="playlist.id"
    :playlist="playlist"
    :cover-url="playlist.coverImgUrl"
    :track-count="playlist.trackCount"
    @close="showEditPlaylist = false"
    @saved="handlePlaylistSaved"
  />
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
  color: var(--sys-text);
}

.create-time {
  color: var(--sys-text-tertiary);
  font-weight: 500;
}

.subscriber-count {
  color: var(--sys-text-secondary);
  padding: 0 4px;
  height: var(--sys-control-height);
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
}

.error-message {
  color: var(--sys-danger);
  padding: 12px 16px;
  border-radius: 12px;
  background: var(--sys-danger-soft);
}

.playlist-view :deep(.action-bar) {
  flex-wrap: wrap;
  gap: 10px;
}

.playlist-view :deep(.play-main-btn) {
  flex-shrink: 0;
  white-space: nowrap;
}

.playlist-view :deep(.search-box-container) {
  flex: 1 1 150px;
  min-width: 120px;
  max-width: 240px;
}
</style>
