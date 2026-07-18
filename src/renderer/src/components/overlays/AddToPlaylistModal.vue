<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { ref, onMounted, reactive } from 'vue'
import { useUserStore } from '@renderer/stores/userStore'
import type { Playlist, PlaylistResponse } from '@renderer/types/userPlaylist'
import type { PlaylistDetail } from '@renderer/types/playlistDetail'

const props = defineProps<{
  songId: number
  songName: string
}>()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const userStore = useUserStore()
const playlists = ref<Playlist[]>([])
const loading = ref(true)
const addingToPlaylist = ref<number | null>(null)
const feedback = ref<{ message: string; type: 'success' | 'error' } | null>(null)
const containsStatus = reactive<Record<number, boolean | null>>({})

onMounted(async () => {
  const uid = userStore.userInfo?.account.id
  if (!uid) {
    loading.value = false
    return
  }

  try {
    const res = (await window.api.user_playlist({ uid })) as { body?: PlaylistResponse }
    if (res.body?.playlist) {
      playlists.value = res.body.playlist
      checkContainsStatus()
    }
  } catch {
    // ignore
  } finally {
    loading.value = false
  }
})

const checkContainsStatus = (): void => {
  for (const pl of playlists.value) {
    containsStatus[pl.id] = null
    void checkPlaylistContains(pl.id)
  }
}

const checkPlaylistContains = async (pid: number): Promise<void> => {
  try {
    const res = (await window.api.playlist_detail({
      id: String(pid),
      s: 0
    })) as { body?: PlaylistDetail }
    const trackIds = res.body?.playlist?.trackIds ?? []
    containsStatus[pid] = trackIds.some((t) => t.id === props.songId)
  } catch {
    containsStatus[pid] = null
  }
}

const isPlaylistContains = (pid: number): boolean | null => containsStatus[pid]

const addToPlaylist = async (pl: Playlist): Promise<void> => {
  if (addingToPlaylist.value !== null) return
  addingToPlaylist.value = pl.id

  try {
    await window.api.playlist_track_add({
      pid: pl.id,
      ids: `[${props.songId}]`,
      cookie: userStore.cookie
    })
    containsStatus[pl.id] = true
    feedback.value = { message: `已添加到「${pl.name}」`, type: 'success' }
    setTimeout(() => emit('close'), 1000)
  } catch {
    feedback.value = { message: '添加失败，请重试', type: 'error' }
  } finally {
    addingToPlaylist.value = null
  }
}

const formatCount = (num: number): string => {
  if (num >= 10000) return (num / 10000).toFixed(1) + '万'
  return num.toString()
}
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" @click.self="emit('close')">
      <div class="add-playlist-card glass-morphism-heavy">
        <div class="card-header">
          <div class="header-content">
            <span class="title">添加到歌单</span>
            <span class="subtitle">「{{ songName }}」</span>
          </div>
          <button class="close-btn" @click="emit('close')">
            <AppIcon name="close" :size="20" />
          </button>
        </div>

        <div v-if="!userStore.isLoggedIn" class="empty-state">
          <div class="empty-icon">♪</div>
          请先登录
        </div>

        <div v-else-if="loading" class="empty-state">
          <div class="spinner"></div>
          加载中...
        </div>

        <div v-else-if="playlists.length === 0" class="empty-state">
          <div class="empty-icon">♪</div>
          暂无歌单
        </div>

        <div v-else class="playlist-list">
          <div
            v-for="pl in playlists"
            :key="pl.id"
            class="playlist-item"
            :class="{
              loading: addingToPlaylist === pl.id,
              'already-added': isPlaylistContains(pl.id) === true,
              'checking': isPlaylistContains(pl.id) === null
            }"
            @click="isPlaylistContains(pl.id) !== true && addToPlaylist(pl)"
          >
            <img
              :src="`${pl.coverImgUrl}?param=80y80`"
              class="playlist-cover"
              alt=""
            />
            <div class="playlist-info">
              <div class="playlist-name">{{ pl.name }}</div>
              <div class="playlist-meta">{{ pl.trackCount }}首 · 播放{{ formatCount(pl.playCount) }}</div>
            </div>
            <div v-if="addingToPlaylist === pl.id" class="adding-spinner"></div>
            <div v-else-if="isPlaylistContains(pl.id) === null" class="checking-spinner"></div>
            <template v-else-if="isPlaylistContains(pl.id) === true">
              <AppIcon name="check" class="check-icon" :size="18" />
              <span class="added-label">已添加</span>
            </template>
            <AppIcon v-else name="plus" class="add-icon" :size="20" />
          </div>
        </div>

        <div v-if="feedback" class="feedback-toast" :class="feedback.type">
          {{ feedback.message }}
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.3);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}

.add-playlist-card {
  width: 380px;
  max-height: 540px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  user-select: none;
  -webkit-app-region: no-drag;
}

.glass-morphism-heavy {
  background: var(--sys-surface);
  backdrop-filter: var(--sys-glass-blur);
  -webkit-backdrop-filter: var(--sys-glass-blur);
  border: 0.5px solid var(--sys-border);
  box-shadow: var(--sys-shadow-elevated);
  border-radius: 28px;
}

.card-header {
  padding: 24px 24px 16px;
  flex-shrink: 0;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
}

.title {
  font-size: 20px;
  font-weight: 800;
  color: var(--sys-text);
  display: block;
}

.subtitle {
  font-size: 12px;
  color: var(--sys-text-tertiary);
  font-weight: 600;
  margin-top: 2px;
  display: block;
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.close-btn {
  background: var(--sys-control);
  border: none;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--sys-text-secondary);
  flex-shrink: 0;
  transition: all 0.2s;
}

.close-btn:hover {
  background: var(--sys-control-hover);
  color: var(--sys-text);
}

.playlist-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 16px 20px;
  scrollbar-gutter: stable;
}

.playlist-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 14px;
  cursor: pointer;
  transition: all 0.2s;
  margin-bottom: 4px;
}

.playlist-item:hover:not(.already-added) {
  background: var(--sys-control);
}

.playlist-item.loading {
  opacity: 0.5;
  pointer-events: none;
}

.playlist-item.already-added {
  cursor: default;
  opacity: 0.55;
}

.playlist-item.checking {
  cursor: default;
}

.playlist-cover {
  width: 44px;
  height: 44px;
  border-radius: 10px;
  object-fit: cover;
  flex-shrink: 0;
}

.playlist-info {
  flex: 1;
  min-width: 0;
}

.playlist-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--sys-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.playlist-meta {
  font-size: 11px;
  color: var(--sys-text-tertiary);
  margin-top: 2px;
}

.add-icon {
  color: var(--sys-text-disabled);
  flex-shrink: 0;
  transition: color 0.2s;
}

.playlist-item:hover:not(.already-added) .add-icon {
  color: var(--theme-color);
}

.check-icon {
  color: var(--sys-success);
  flex-shrink: 0;
}

.added-label {
  font-size: 11px;
  color: var(--sys-text-disabled);
  font-weight: 600;
}

.adding-spinner {
  width: 18px;
  height: 18px;
  border: 2px solid var(--sys-control-hover);
  border-top-color: var(--theme-color);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  flex-shrink: 0;
}

.checking-spinner {
  width: 16px;
  height: 16px;
  border: 2px solid var(--sys-control);
  border-top-color: var(--sys-text-disabled);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  flex-shrink: 0;
}

.empty-state {
  padding: 40px 20px 60px;
  text-align: center;
  color: var(--sys-text-disabled);
  font-size: 14px;
}

.empty-icon {
  font-size: 40px;
  margin-bottom: 12px;
}

.spinner {
  width: 24px;
  height: 24px;
  border: 2px solid var(--sys-control-hover);
  border-top-color: var(--theme-color);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin: 0 auto 12px;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.feedback-toast {
  padding: 10px 20px;
  margin: 0 16px 16px;
  border-radius: 12px;
  font-size: 13px;
  text-align: center;
  font-weight: 600;
}

.feedback-toast.success {
  background: var(--sys-success-soft);
  color: var(--sys-success);
}

.feedback-toast.error {
  background: var(--sys-danger-soft);
  color: var(--sys-danger);
}
</style>
