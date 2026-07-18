<script setup lang="ts">
import AppIcon from './AppIcon.vue'
import { computed, ref } from 'vue'
import { usePlayerStore } from '@renderer/stores/playerStore'
import SongCover from './SongCover.vue'
import SongContextMenu from './SongContextMenu.vue'
import { Song } from '@renderer/types/songDetail'
import { CurrentSong, createCurrentSongArtists } from '@renderer/stores/playerStore'
import { useFavoriteStore } from '@renderer/stores/favoriteStore'

const props = defineProps<{
  songs: Song[]
  searchQuery?: string
  fallbackCover?: string
  playlistId?: number
  variant?: 'cloud' | 'local'
}>()

const emit = defineEmits<{
  (e: 'play', song: Song): void
  (e: 'removed', songId: number): void
}>()

const playerStore = usePlayerStore()
const favoriteStore = useFavoriteStore()
const openMenuSongId = ref<number | null>(null)
const isLocalVariant = computed(() => props.variant === 'local')

const formatDuration = (ms: number): string => {
  if (!Number.isFinite(ms) || ms <= 0) return '--:--'
  const totalSeconds = Math.floor(ms / 1000)
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}:${seconds.toString().padStart(2, '0')}`
}

const handlePlaySong = (song: Song): void => {
  emit('play', song)
}

const mapSongToCurrentSong = (song: Song): CurrentSong => ({
  id: song.id,
  name: song.name,
  artists: createCurrentSongArtists(song.ar),
  cover: song.al?.picUrl || props.fallbackCover || '',
  duration: song.dt
})

const toggleFavorite = (song: Song): void => {
  void favoriteStore.toggleFavorite(mapSongToCurrentSong(song))
}
</script>

<template>
  <section class="tracks-section">
    <div class="list-header-sticky">
      <div class="list-header-content">
        <div class="col-index">#</div>
        <div class="col-title">标题</div>
        <div class="col-album">{{ isLocalVariant ? '文件' : '专辑' }}</div>
        <div class="col-time">
          <AppIcon name="clock" :size="14" />
        </div>
      </div>
    </div>

    <div class="tracks-list">
      <div
        v-for="(track, index) in songs"
        :key="track.id"
        class="track-row"
        :class="{ 'is-active': track.id === playerStore.currentSongId }"
        @dblclick="handlePlaySong(track)"
      >
        <div class="col-index">
          <span class="index-num">{{ (index + 1).toString().padStart(2, '0') }}</span>
          <AppIcon name="play" class="play-icon" :size="16" />
        </div>

        <div class="col-title">
          <div class="mini-cover-wrapper">
            <div v-if="isLocalVariant" class="local-cover-icon" aria-hidden="true">
              <AppIcon name="music" :size="20" />
            </div>
            <SongCover v-else :id="track.al?.picUrl || fallbackCover" size="80y80" />
          </div>
          <div class="song-info">
            <span class="song-name">{{ track.name }}</span>
            <span class="song-artist">
              {{ track.ar.map((a) => a.name).join(' / ') }}
            </span>
          </div>
        </div>

        <div class="col-album">
          <span v-if="isLocalVariant" class="album-name" :title="track.al?.name">
            {{ track.al?.name }}
          </span>
          <router-link v-else :to="`/album/${track.al?.id}`" class="album-name">
            {{ track.al?.name }}
          </router-link>
        </div>

        <div class="col-time">
          <button
            v-if="!isLocalVariant"
            class="favorite-btn"
            :class="{ active: favoriteStore.isFavorite(track.id) }"
            :title="favoriteStore.isFavorite(track.id) ? '取消喜欢' : '喜欢'"
            @click.stop="toggleFavorite(track)"
          >
            <AppIcon
              :name="favoriteStore.isFavorite(track.id) ? 'heart-fill' : 'heart'"
              :size="16"
            />
          </button>
          <span class="duration-text">{{ formatDuration(track.dt) }}</span>
          <div v-if="!isLocalVariant" class="row-more-wrapper">
            <button
              class="row-more"
              :class="{ 'menu-open': openMenuSongId === track.id }"
              @click.stop="openMenuSongId = openMenuSongId === track.id ? null : track.id"
            >
              <AppIcon name="more-dots" :size="16" />
            </button>
            <SongContextMenu
              v-if="openMenuSongId === track.id"
              :song-id="track.id"
              :song-name="track.name"
              :playlist-id="playlistId"
              :show-remove="playlistId !== undefined"
              @close="openMenuSongId = null"
              @removed="emit('removed', track.id)"
            />
          </div>
          <button
            v-else
            class="remove-local-btn"
            type="button"
            :title="`将 ${track.name} 移出曲库`"
            @click.stop="emit('removed', track.id)"
          >
            <AppIcon name="trash" :size="16" />
          </button>
        </div>
      </div>
    </div>

    <div v-if="songs.length === 0" class="no-results">
      <template v-if="searchQuery">没有找到匹配 "{{ searchQuery }}" 的歌曲</template>
      <template v-else>暂无歌曲</template>
    </div>
  </section>
</template>

<style scoped>
.tracks-section {
  -webkit-app-region: no-drag;
}

.list-header-sticky {
  position: sticky;
  top: 0;
  z-index: 10;
  backdrop-filter: blur(15px);
  -webkit-backdrop-filter: blur(15px);
  margin: 0 -40px;
  padding: 0 40px;
}

.list-header-content {
  display: flex;
  padding: 14px 16px;
  border-bottom: 1px solid var(--sys-border);
  color: var(--sys-text-tertiary);
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
}

.track-row {
  display: flex;
  align-items: center;
  padding: 8px 16px;
  border-radius: 10px;
  cursor: pointer;
  transition: background 0.2s;
  margin: 2px 0;
}

.track-row:hover {
  background: var(--sys-control);
}

.track-row.is-active {
  background: var(--sys-control-active);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.5);
}

.track-row.is-active .song-name {
  color: var(--theme-color-strong);
}

.col-index {
  width: 40px;
  color: var(--sys-text-disabled);
}
.col-title {
  flex: 3;
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
}
.col-album {
  flex: 2;
  font-size: 13px;
  color: var(--sys-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.col-time {
  width: 116px;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  color: var(--sys-text-tertiary);
  font-size: 12px;
}

.play-icon {
  display: none;
  width: 14px;
  height: 14px;
  color: var(--theme-color-strong);
}
.track-row:hover .index-num {
  display: none;
}
.track-row:hover .play-icon {
  display: block;
}

.mini-cover-wrapper {
  width: 40px;
  height: 40px;
  border-radius: 6px;
  overflow: hidden;
  background: var(--sys-control);
  flex-shrink: 0;
}

.local-cover-icon {
  width: 100%;
  height: 100%;
  display: grid;
  place-items: center;
  color: var(--theme-color-strong);
}

.local-cover-icon svg {
  width: 20px;
  height: 20px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.song-info {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.song-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--sys-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.song-artist {
  font-size: 12px;
  color: var(--sys-text-tertiary);
}
.album-name {
  flex: 2;
  font-size: 13px;
  color: var(--sys-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-more-wrapper {
  position: relative;
  display: flex;
}
.row-more {
  display: none;
  background: none;
  border: none;
  color: var(--sys-text-disabled);
  cursor: pointer;
}
.row-more.menu-open {
  display: block;
  color: var(--sys-text);
}
.remove-local-btn {
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--sys-text-disabled);
  cursor: pointer;
  opacity: 0;
  transition:
    color 0.2s,
    opacity 0.2s,
    background 0.2s;
}
.remove-local-btn svg {
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.remove-local-btn:hover {
  background: var(--sys-danger-soft);
  color: var(--sys-danger);
}
.favorite-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--sys-text-disabled);
  cursor: pointer;
  opacity: 0;
  transition:
    color 0.2s,
    opacity 0.2s,
    background 0.2s;
}
.favorite-btn svg {
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.favorite-btn:hover {
  background: var(--sys-control-hover);
  color: var(--theme-color-strong);
}
.favorite-btn.active {
  color: var(--theme-color-strong);
  opacity: 1;
}
.favorite-btn.active svg {
  fill: currentColor;
}
.track-row:hover .favorite-btn {
  opacity: 1;
}
.track-row:hover .row-more {
  display: block;
}
.track-row:hover .remove-local-btn,
.remove-local-btn:focus-visible {
  opacity: 1;
}

.no-results {
  padding: 40px;
  text-align: center;
  color: var(--sys-text-tertiary);
  font-size: 14px;
}

@media (max-width: 900px) {
  .col-album {
    display: none;
  }
}
</style>
