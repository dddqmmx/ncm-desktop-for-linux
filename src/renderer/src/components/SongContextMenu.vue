<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { useUserStore } from '@renderer/stores/userStore'
import type { Playlist, PlaylistResponse } from '@renderer/types/userPlaylist'

const props = defineProps<{
  songId: number
  songName: string
  playlistId?: number
  showRemove?: boolean
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'removed'): void
}>()

const userStore = useUserStore()
const playlists = ref<Playlist[]>([])
const showPlaylistSubmenu = ref(false)
const addingToPlaylist = ref<number | null>(null)
const removingSong = ref(false)
const feedbackMessage = ref('')
const feedbackType = ref<'success' | 'error'>('success')
const isLoggedIn = computed(() => userStore.isLoggedIn)

const fetchPlaylists = async (): Promise<void> => {
  const uid = userStore.userInfo?.account.id
  if (!uid) return

  try {
    const res = (await window.api.user_playlist({ uid })) as { body?: PlaylistResponse }
    if (res.body?.playlist) {
      playlists.value = res.body.playlist
    }
  } catch {
    // ignore
  }
}

const addToPlaylist = async (pid: number, playlistName: string): Promise<void> => {
  if (addingToPlaylist.value !== null) return
  addingToPlaylist.value = pid

  try {
    await window.api.playlist_track_add({
      pid,
      ids: `[${props.songId}]`,
      cookie: userStore.cookie
    })
    feedbackMessage.value = `已将「${props.songName}」添加到歌单「${playlistName}」`
    feedbackType.value = 'success'
    setTimeout(() => emit('close'), 800)
  } catch {
    feedbackMessage.value = '添加失败，请重试'
    feedbackType.value = 'error'
  } finally {
    addingToPlaylist.value = null
  }
}

const removeFromPlaylist = async (): Promise<void> => {
  if (!props.playlistId || removingSong.value) return
  removingSong.value = true

  try {
    await window.api.playlist_track_delete({
      pid: props.playlistId,
      ids: `[${props.songId}]`,
      cookie: userStore.cookie
    })
    feedbackMessage.value = `已将「${props.songName}」从歌单中删除`
    feedbackType.value = 'success'
    emit('removed')
    setTimeout(() => emit('close'), 800)
  } catch {
    feedbackMessage.value = '删除失败，请重试'
    feedbackType.value = 'error'
  } finally {
    removingSong.value = false
  }
}

const handleClickOutside = (e: MouseEvent): void => {
  const target = e.target as HTMLElement
  if (!target.closest('.song-context-menu')) {
    emit('close')
  }
}

const handleEscape = (e: KeyboardEvent): void => {
  if (e.key === 'Escape') {
    emit('close')
  }
}

onMounted(() => {
  if (isLoggedIn.value) {
    fetchPlaylists()
  }
  document.addEventListener('click', handleClickOutside, true)
  document.addEventListener('keydown', handleEscape)
})

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside, true)
  document.removeEventListener('keydown', handleEscape)
})
</script>

<template>
  <div class="song-context-menu" @click.stop>
    <template v-if="!isLoggedIn">
      <div class="menu-hint">请先登录</div>
    </template>
    <template v-else>
      <div
        class="menu-item"
        :class="{ expanded: showPlaylistSubmenu }"
        @click="showPlaylistSubmenu = !showPlaylistSubmenu"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" class="menu-icon">
          <path
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M12 5v14M5 12h14"
          />
        </svg>
        <span>添加到歌单</span>
        <svg viewBox="0 0 24 24" width="12" height="12" class="arrow-icon">
          <path
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M9 18l6-6-6-6"
          />
        </svg>
      </div>

      <div v-if="showPlaylistSubmenu" class="playlist-submenu">
        <div v-if="playlists.length === 0" class="submenu-empty">暂无歌单</div>
        <div
          v-for="pl in playlists"
          :key="pl.id"
          class="submenu-item"
          :class="{ loading: addingToPlaylist === pl.id }"
          @click="addToPlaylist(pl.id, pl.name)"
        >
          <img :src="`${pl.coverImgUrl}?param=40y40`" class="submenu-cover" alt="" />
          <span class="submenu-name">{{ pl.name }}</span>
          <span class="submenu-count">{{ pl.trackCount }}首</span>
        </div>
      </div>

      <div v-if="showRemove" class="menu-divider"></div>
      <div
        v-if="showRemove"
        class="menu-item remove-item"
        :class="{ loading: removingSong }"
        @click="removeFromPlaylist"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" class="menu-icon">
          <path
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M3 6h18M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"
          />
        </svg>
        <span>从歌单删除</span>
      </div>

      <div v-if="feedbackMessage" class="feedback" :class="feedbackType">
        {{ feedbackMessage }}
      </div>
    </template>
  </div>
</template>

<style scoped>
.song-context-menu {
  position: absolute;
  right: 0;
  top: 100%;
  z-index: 1000;
  min-width: 220px;
  max-width: 280px;
  background: var(--sys-surface);
  backdrop-filter: var(--sys-glass-blur);
  -webkit-backdrop-filter: var(--sys-glass-blur);
  border: 0.5px solid var(--sys-border);
  border-radius: 14px;
  box-shadow: var(--sys-shadow-elevated);
  padding: 6px;
  margin-top: 4px;
}

.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 10px;
  cursor: pointer;
  font-size: 13px;
  color: var(--sys-text);
  transition: background 0.15s;
}

.menu-item:hover {
  background: var(--sys-control);
}

.menu-item.expanded {
  background: var(--sys-control-active);
}

.menu-icon {
  flex-shrink: 0;
  color: var(--sys-text-tertiary);
}

.arrow-icon {
  margin-left: auto;
  color: var(--sys-text-disabled);
  transition: transform 0.2s;
}

.menu-item.expanded .arrow-icon {
  transform: rotate(90deg);
}

.remove-item:hover {
  background: var(--sys-danger-soft);
  color: var(--sys-danger);
}

.remove-item:hover .menu-icon {
  color: var(--sys-danger);
}

.menu-divider {
  height: 1px;
  background: var(--sys-border);
  margin: 4px 10px;
}

.menu-hint {
  padding: 12px 16px;
  font-size: 13px;
  color: var(--sys-text-tertiary);
  text-align: center;
}

.adding-icon {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.playlist-submenu {
  max-height: 260px;
  overflow-y: auto;
  padding: 2px 0;
}

.submenu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px 6px 32px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 12px;
  color: var(--sys-text-secondary);
  transition: background 0.15s;
}

.submenu-item:hover {
  background: var(--sys-control);
}

.submenu-item.loading {
  opacity: 0.5;
  pointer-events: none;
}

.submenu-cover {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  object-fit: cover;
  flex-shrink: 0;
}

.submenu-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.submenu-count {
  font-size: 11px;
  color: var(--sys-text-disabled);
  flex-shrink: 0;
}

.submenu-empty {
  padding: 12px 10px 12px 32px;
  font-size: 12px;
  color: var(--sys-text-disabled);
}

.feedback {
  padding: 6px 10px;
  border-radius: 8px;
  font-size: 12px;
  text-align: center;
  margin-top: 4px;
}

.feedback.success {
  background: var(--sys-success-soft);
  color: var(--sys-success);
}

.feedback.error {
  background: var(--sys-danger-soft);
  color: var(--sys-danger);
}
</style>
