<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import SongCover from './SongCover.vue'
import { Song } from '@renderer/types/songDetail'

defineProps<{
  songs: Song[]
  searchQuery?: string
  fallbackCover?: string
}>()

const emit = defineEmits<{
  (e: 'play', song: Song): void
}>()

const playerStore = usePlayerStore()

const formatDuration = (ms: number): string => {
  const totalSeconds = Math.floor(ms / 1000)
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}:${seconds.toString().padStart(2, '0')}`
}

const handlePlaySong = (song: Song): void => {
  emit('play', song)
}
</script>

<template>
  <section class="tracks-section">
    <div class="list-header-sticky">
      <div class="list-header-content">
        <div class="col-index">#</div>
        <div class="col-title">标题</div>
        <div class="col-album">专辑</div>
        <div class="col-time">
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <circle cx="12" cy="12" r="10" />
            <polyline points="12 6 12 12 16 14" />
          </svg>
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
          <svg class="play-icon" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 5v14l11-7z" />
          </svg>
        </div>

        <div class="col-title">
          <div class="mini-cover-wrapper">
            <SongCover :id="track.al?.picUrl || fallbackCover" size="80y80" />
          </div>
          <div class="song-info">
            <span class="song-name">{{ track.name }}</span>
            <span class="song-artist">
              {{ track.ar.map((a) => a.name).join(' / ') }}
            </span>
          </div>
        </div>

        <div class="col-album">
          <router-link :to="`/album/${track.al?.id}`" class="album-name">
            {{ track.al?.name }}
          </router-link>
        </div>

        <div class="col-time">
          <span class="duration-text">{{ formatDuration(track.dt) }}</span>
          <button class="row-more">
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="12" cy="12" r="1" />
              <circle cx="19" cy="12" r="1" />
              <circle cx="5" cy="12" r="1" />
            </svg>
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
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);
  color: #aaa;
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
  background: rgba(0, 0, 0, 0.04);
}

.track-row.is-active {
  background: rgba(0, 0, 0, 0.08);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.5);
}

.track-row.is-active .song-name {
  color: #111;
}

.col-index {
  width: 40px;
  color: #ccc;
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
  color: #888;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.col-time {
  width: 80px;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  color: #aaa;
  font-size: 12px;
}

.play-icon {
  display: none;
  width: 14px;
  height: 14px;
  color: #111;
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
  background: rgba(0, 0, 0, 0.05);
  flex-shrink: 0;
}

.song-info {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.song-name {
  font-size: 14px;
  font-weight: 500;
  color: #222;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.song-artist {
  font-size: 12px;
  color: #999;
}
.album-name {
  flex: 2;
  font-size: 13px;
  color: #888;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-more {
  display: none;
  background: none;
  border: none;
  color: #ccc;
  cursor: pointer;
}
.track-row:hover .row-more {
  display: block;
}

.no-results {
  padding: 40px;
  text-align: center;
  color: #999;
  font-size: 14px;
}

@media (max-width: 900px) {
  .col-album {
    display: none;
  }
}
</style>
