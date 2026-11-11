<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import { useUserStore } from '@renderer/stores/userStore'
import { usePlayerStore } from '@renderer/stores/playerStore'
import { useDownloadStore } from '@renderer/stores/downloadStore'
import { useConfigStore } from '@renderer/stores/configStore'
import {
  DOWNLOAD_QUALITIES,
  preferredDownloadQuality,
  type DownloadQuality
} from '../../../../shared/download'
import type { CurrentSong } from '@renderer/types/player'
import type { Playlist, PlaylistResponse } from '@renderer/types/userPlaylist'
import {
  apiErrorMessage,
  isApiSuccess,
  isDuplicateTrackResult,
  type ServiceResult
} from '@renderer/utils/api'

const props = defineProps<{
  song: CurrentSong
  playlistId?: number
  showRemove?: boolean
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'removed'): void
}>()

const userStore = useUserStore()
const playerStore = usePlayerStore()
const downloadStore = useDownloadStore()
const configStore = useConfigStore()
const downloadState = computed(() => downloadStore.downloads[props.song.id])
const downloadResult = computed(() => downloadState.value?.result)
const qualityState = computed(() => downloadStore.qualityOptions[props.song.id])
const qualityResult = computed(() => qualityState.value?.result)
const qualityOptions = computed(() =>
  qualityResult.value?.status === 'ready' ? qualityResult.value.options : []
)
const displayedQualities = computed(() =>
  DOWNLOAD_QUALITIES.map((item) => ({
    ...item,
    ...qualityOptions.value.find((option) => option.quality === item.quality)
  }))
)
const defaultQuality = computed(() =>
  preferredDownloadQuality(qualityOptions.value, configStore.soundQuality)
)
const selectedQuality = ref<DownloadQuality | null>(null)
const canDownload = computed(
  () =>
    !qualityState.value?.loading &&
    !downloadState.value?.pending &&
    qualityOptions.value.some(
      (option) => option.quality === selectedQuality.value && option.available
    )
)
const completedQualityLabel = computed(() => {
  const result = downloadResult.value
  return result?.status === 'completed'
    ? DOWNLOAD_QUALITIES.find((item) => item.quality === result.quality)?.label
    : ''
})
const playlists = ref<Playlist[]>([])
const showPlaylistSubmenu = ref(false)
const showDownloadSubmenu = ref(false)
const addingToPlaylist = ref<number | null>(null)
const removingSong = ref(false)
const feedbackMessage = ref('')
const feedbackType = ref<'success' | 'error'>('success')
const isLoggedIn = computed(() => userStore.isLoggedIn)

watch(defaultQuality, (quality) => {
  selectedQuality.value = quality
})
watch([() => props.song.id, () => userStore.cookie], () => {
  selectedQuality.value = null
  if (showDownloadSubmenu.value) void downloadStore.loadQualities(props.song.id)
})

const toggleDownloadSubmenu = (): void => {
  showDownloadSubmenu.value = !showDownloadSubmenu.value
  if (showDownloadSubmenu.value) {
    showPlaylistSubmenu.value = false
    void downloadStore.loadQualities(props.song.id)
  }
}

const togglePlaylistSubmenu = (): void => {
  showPlaylistSubmenu.value = !showPlaylistSubmenu.value
  showDownloadSubmenu.value = false
}

const addNext = (): void => {
  if (!playerStore.addNext(props.song)) return
  feedbackMessage.value = `已将「${props.song.name}」添加到下一曲`
  feedbackType.value = 'success'
}

const downloadSong = (): void => {
  if (!canDownload.value || !selectedQuality.value) return
  feedbackMessage.value = ''
  void downloadStore.download(props.song, selectedQuality.value)
}

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
    const res = (await window.api.playlist_track_add({
      pid,
      ids: `[${props.song.id}]`,
      cookie: userStore.cookie
    })) as ServiceResult

    if (isDuplicateTrackResult(res)) {
      feedbackMessage.value = `歌单「${playlistName}」里已经有「${props.song.name}」了`
      feedbackType.value = 'error'
      return
    }

    if (!isApiSuccess(res)) {
      feedbackMessage.value = apiErrorMessage(res, '添加失败，请重试')
      feedbackType.value = 'error'
      return
    }

    feedbackMessage.value = `已将「${props.song.name}」添加到歌单「${playlistName}」`
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
    const res = (await window.api.playlist_track_delete({
      pid: props.playlistId,
      ids: `[${props.song.id}]`,
      cookie: userStore.cookie
    })) as ServiceResult

    if (!isApiSuccess(res)) {
      feedbackMessage.value = apiErrorMessage(res, '删除失败，请重试')
      feedbackType.value = 'error'
      return
    }

    feedbackMessage.value = `已将「${props.song.name}」从歌单中删除`
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
  <div class="song-context-menu" @click.stop @dblclick.stop>
    <button
      type="button"
      class="menu-item"
      :disabled="song.id === playerStore.currentSongId"
      :title="song.id === playerStore.currentSongId ? '当前正在播放这首歌' : undefined"
      @click="addNext"
    >
      <AppIcon name="next" class="menu-icon" :size="14" />
      <span>添加到下一曲</span>
    </button>
    <button
      type="button"
      class="menu-item"
      :class="{ expanded: showDownloadSubmenu }"
      :disabled="downloadState?.pending"
      :aria-expanded="showDownloadSubmenu"
      @click="toggleDownloadSubmenu"
    >
      <AppIcon name="download" class="menu-icon" :size="14" />
      <span>{{ downloadState?.pending ? '下载中…' : '下载' }}</span>
      <AppIcon name="chevron-right" class="arrow-icon" :size="12" />
    </button>
    <div v-if="showDownloadSubmenu" class="download-submenu">
      <div class="quality-heading">
        <span>选择下载音质</span>
        <button
          type="button"
          class="quality-refresh"
          :disabled="qualityState?.loading || downloadState?.pending"
          @click="downloadStore.loadQualities(song.id)"
        >
          刷新
        </button>
      </div>
      <div v-if="qualityState?.loading" class="quality-hint" role="status">
        正在读取账号下载权限…
      </div>
      <div v-else-if="qualityResult?.status === 'error'" class="feedback error" role="status">
        {{ qualityResult.message }}
      </div>
      <template v-else-if="qualityResult?.status === 'ready'">
        <div class="quality-hint">已按当前账号的会员及歌曲权限筛选</div>
        <div class="quality-options">
          <label
            v-for="option in displayedQualities"
            :key="option.quality"
            class="quality-option"
            :class="{
              unavailable: !option.available,
              selected: selectedQuality === option.quality
            }"
          >
            <input
              v-model="selectedQuality"
              type="radio"
              :name="`download-quality-${song.id}`"
              :value="option.quality"
              :disabled="!option.available || downloadState?.pending"
            />
            <span class="quality-copy">
              <span class="quality-label">
                {{ option.label }}
                <span v-if="option.quality === defaultQuality" class="quality-default">默认</span>
              </span>
              <span class="quality-description">{{ option.reason || option.description }}</span>
            </span>
            <AppIcon v-if="!option.available" name="lock" class="menu-icon" :size="12" />
          </label>
        </div>
        <div v-if="!defaultQuality" class="quality-hint" role="status">当前账号暂无可下载音质</div>
        <button
          type="button"
          class="quality-download"
          :disabled="!canDownload"
          @click="downloadSong"
        >
          {{ downloadState?.pending ? '下载中…' : '下载所选音质' }}
        </button>
      </template>
    </div>
    <div class="menu-divider"></div>
    <template v-if="!isLoggedIn">
      <div class="menu-hint">登录后可添加到歌单</div>
    </template>
    <template v-else>
      <div
        class="menu-item"
        :class="{ expanded: showPlaylistSubmenu }"
        @click="togglePlaylistSubmenu"
      >
        <AppIcon name="plus" class="menu-icon" :size="14" />
        <span>添加到歌单</span>
        <AppIcon name="chevron-right" class="arrow-icon" :size="12" />
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
        <AppIcon name="trash-outline" class="menu-icon" :size="14" />
        <span>从歌单删除</span>
      </div>
    </template>
    <div v-if="feedbackMessage" class="feedback" :class="feedbackType" role="status">
      {{ feedbackMessage }}
    </div>
    <div v-if="downloadResult?.status === 'completed'" class="feedback success" role="status">
      {{ completedQualityLabel }}已保存到 {{ downloadResult.filePath }}
    </div>
    <div v-else-if="downloadResult?.status === 'error'" class="feedback error" role="status">
      {{ downloadResult.message }}
    </div>
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
  background: var(--glass-fill);
  backdrop-filter: var(--sys-glass-blur);
  -webkit-backdrop-filter: var(--sys-glass-blur);
  border: var(--glass-border-width-hairline) solid var(--glass-border);
  border-radius: 14px;
  box-shadow:
    var(--glass-shadow-elevated),
    inset 0 1px 0 var(--glass-highlight),
    inset 0 -1px 0 var(--glass-shade);
  padding: 6px;
  margin-top: 4px;
}

.menu-item {
  width: 100%;
  border: 0;
  background: transparent;
  font-family: inherit;
  text-align: left;
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

.menu-item:disabled {
  opacity: 0.5;
  cursor: default;
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

.download-submenu {
  padding: 6px 8px;
}

.quality-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--sys-text);
  font-size: 12px;
}

.quality-refresh {
  border: 0;
  background: transparent;
  color: var(--theme-color-strong);
  cursor: pointer;
  font: inherit;
  padding: 4px;
}

.quality-hint {
  margin: 4px 0 8px;
  font-size: 11px;
  color: var(--sys-text-tertiary);
}

.quality-options {
  max-height: 240px;
  overflow-y: auto;
}

.quality-option {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px;
  border-radius: 8px;
  cursor: pointer;
  color: var(--sys-text);
}

.quality-option:hover:not(.unavailable),
.quality-option.selected {
  background: var(--sys-control-active);
}

.quality-option.unavailable {
  color: var(--sys-text-tertiary);
  cursor: not-allowed;
}

.quality-option input {
  margin: 0;
  accent-color: var(--theme-color-strong);
}

.quality-copy {
  display: flex;
  flex-direction: column;
  flex: 1;
  gap: 2px;
  min-width: 0;
}

.quality-label {
  font-size: 12px;
}

.quality-default {
  margin-left: 4px;
  color: var(--theme-color-strong);
  font-size: 10px;
}

.quality-description {
  font-size: 10px;
  color: var(--sys-text-tertiary);
}

.quality-download {
  margin-top: 8px;
  width: 100%;
  border: 0;
  border-radius: 8px;
  background: var(--sys-control-active);
  color: var(--theme-color-strong);
  cursor: pointer;
  font: inherit;
  font-size: 12px;
  padding: 8px;
}

.quality-download:disabled,
.quality-refresh:disabled {
  opacity: 0.5;
  cursor: default;
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
  overflow-wrap: anywhere;
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
