<script setup lang="ts">
import { useRoute } from 'vue-router'
import { ref, computed, watch } from 'vue'
import { CurrentSong, createCurrentSongArtists, usePlayerStore } from '@renderer/stores/playerStore'
import { AlbumDetail, AlbumDetailInfo } from '@renderer/types/album'
import { Song } from '@renderer/types/songDetail'
import AlbumCover from '../components/AlbumCover.vue'
import SongCover from '../components/SongCover.vue'

const route = useRoute()

// --- 响应式数据 ---
const album = ref<AlbumDetailInfo | null>(null)
const tracks = ref<Song[]>([])
const loading = ref(true)
const searchQuery = ref('')

// --- 格式化工具 ---
const formatDuration = (ms: number): string => {
  const totalSeconds = Math.floor(ms / 1000)
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}:${seconds.toString().padStart(2, '0')}`
}

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
  // 优先使用专辑高清封面 (400x400)，否则退而求其次使用轨道封面
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
      track.name.toLowerCase().includes(query) || // 搜索歌名
      (track.al?.name || '').toLowerCase().includes(query) || // 搜索专辑名
      track.ar.some((artist) => artist.name.toLowerCase().includes(query)) // 搜索歌手名
    )
  })
})

const playerStore = usePlayerStore()
</script>

<template>
  <div class="main-content-scroll-wrapper">
    <!-- 加载状态 -->
    <div v-if="loading" class="loading-state">
      <div class="spinner"></div>
    </div>

    <div v-else-if="album" class="playlist-container">
      <!-- 专辑头部 -->
      <header class="playlist-header">
        <div class="cover-wrapper">
          <AlbumCover :id="album.picUrl" :alt="album.name" size="400y400" />
        </div>
        <div class="playlist-details">
          <div class="playlist-type-tag">ALBUM</div>
          <h1 class="playlist-title">{{ album.name }}</h1>

          <div class="creator-info">
            <span class="creator-name">{{ album.artists.map((a) => a.name).join(' / ') }}</span>
            <span class="create-time">{{ formatDate(album.publishTime) }} 发行</span>
          </div>

          <p v-if="album.description" class="playlist-description">
            {{ album.description }}
          </p>

          <div class="playlist-meta">
            <span>{{ album.size }} 首歌曲</span>
            <span v-if="album.company" class="dot"></span>
            <span v-if="album.company">{{ album.company }}</span>
          </div>

          <div class="action-bar">
            <button class="play-main-btn" @click="handlePlayAll()">
              <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
                <path d="M8 5v14l11-7z" />
              </svg>
              播放全部
            </button>
            <div class="search-box-container">
              <svg
                class="search-icon"
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
              >
                <circle cx="11" cy="11" r="8" />
                <path d="m21 21-4.3-4.3" />
              </svg>
              <input
                v-model="searchQuery"
                type="text"
                placeholder="在专辑内搜索..."
                class="search-input"
              />
              <button v-if="searchQuery" class="clear-btn" @click="searchQuery = ''">
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>
            <button class="icon-btn">
              <svg
                width="20"
                height="20"
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
      </header>

      <!-- 歌曲列表 -->
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
            v-for="(track, index) in filteredTracks"
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
                <SongCover :id="track.al.picUrl || album.picUrl" size="80y80" />
              </div>
              <div class="song-info">
                <span class="song-name">{{ track.name }}</span>
                <span class="song-artist">
                  {{ track.ar.map((a) => a.name).join(' / ') }}
                </span>
              </div>
            </div>

            <div class="col-album">
              <router-link :to="`/album/${track.al.id}`" class="album-name">
                {{ track.al.name }}
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

        <div v-if="filteredTracks.length === 0" class="no-results">
          没有找到匹配 "{{ searchQuery }}" 的歌曲
        </div>
      </section>

      <div class="spacer-bottom"></div>
    </div>
  </div>
</template>

<style scoped>
/* 核心容器：去除背景色 */
.main-content-scroll-wrapper {
  height: 100%;
  overflow-y: auto;
  scrollbar-width: thin;
}

/* 隐藏滚动条但保留功能 */
.main-content-scroll-wrapper::-webkit-scrollbar {
  width: 4px;
}

.main-content-scroll-wrapper::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.1);
  border-radius: 10px;
}

.playlist-container {
  padding: 32px 40px;
  max-width: 1200px;
  margin: 0 auto;
  -webkit-app-region: no-drag;
}

.action-bar,
.action-bar *,
.tracks-section,
.tracks-section * {
  -webkit-app-region: no-drag;
}

/* --- Header 部分 --- */
.playlist-header {
  display: flex;
  gap: 36px;
  align-items: flex-end;
  margin-bottom: 40px;
}

.cover-wrapper {
  width: 220px;
  height: 220px;
  flex-shrink: 0;
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 15px 40px rgba(0, 0, 0, 0.15); /* 加强阴影以在无背景时突出 */
}

.playlist-details {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.playlist-type-tag {
  font-size: 11px;
  font-weight: 800;
  letter-spacing: 1.5px;
  color: rgba(0, 0, 0, 0.5);
}

.playlist-title {
  font-size: 42px;
  font-weight: 900;
  margin: 0;
  color: #111;
  letter-spacing: -1px;
}

.creator-info {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 14px;
}

.creator-avatar {
  width: 26px;
  height: 26px;
  border-radius: 50%;
}

.creator-name {
  font-weight: 600;
  color: #333;
}

.create-time {
  color: #999;
}

.playlist-description {
  font-size: 13px;
  color: #666;
  line-height: 1.5;
  margin: 4px 0;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.playlist-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
  color: #888;
}

.dot {
  width: 3px;
  height: 3px;
  background: #ccc;
  border-radius: 50%;
}

/* --- 按钮交互 --- */
.action-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 16px;
}

.play-main-btn {
  background: #111;
  color: #fff;
  border: none;
  padding: 10px 24px;
  border-radius: 100px;
  font-weight: 700;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  transition: transform 0.2s;
}

.play-main-btn:hover {
  transform: scale(1.03);
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

.icon-btn {
  background: transparent;
  border: 1px solid rgba(0, 0, 0, 0.1);
  width: 38px;
  height: 38px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: #666;
}

/* --- 歌曲列表：透明化设计 --- */
.list-header-sticky {
  position: sticky;
  top: 0;
  z-index: 10;
  /* 使用毛玻璃效果代替背景色 */
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

/* 激活态仅通过轻微遮罩和文字变色区分 */
.track-row.is-active {
  background: rgba(0, 0, 0, 0.08); /* 稍微深一点点 */
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.5); /* 内部顶部微光，增加立体感 */
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

.loading-state {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 300px;
}

.spinner {
  width: 30px;
  height: 30px;
  border: 3px solid rgba(0, 0, 0, 0.1);
  border-top-color: #111;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

/* 搜索框容器 */
.search-box-container {
  position: relative;
  display: flex;
  align-items: center;
  margin-left: auto; /* 将搜索框推向右侧 */
  background: rgba(0, 0, 0, 0.05);
  border-radius: 8px;
  padding: 0 12px;
  transition: all 0.3s ease;
  width: 200px;
}

.search-box-container:focus-within {
  background: rgba(0, 0, 0, 0.08);
  width: 260px; /* 聚焦时变长 */
  box-shadow: 0 0 0 2px rgba(0, 0, 0, 0.05);
}

.search-icon {
  color: #999;
  margin-right: 8px;
}

.search-input {
  background: transparent;
  border: none;
  outline: none;
  height: 36px;
  width: 100%;
  font-size: 13px;
  color: #333;
}

.clear-btn {
  background: none;
  border: none;
  color: #999;
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
}

.clear-btn:hover {
  color: #333;
}

/* 无结果样式 */
.no-results {
  padding: 40px;
  text-align: center;
  color: #999;
  font-size: 14px;
}

/* 移动端适配修改 */
@media (max-width: 900px) {
  .search-box-container {
    width: 100%;
    margin-left: 0;
    margin-top: 10px;
  }
  .action-bar {
    flex-wrap: wrap;
  }
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.spacer-bottom {
  height: 80px;
}

@media (max-width: 900px) {
  .col-album {
    display: none;
  }
  .playlist-header {
    flex-direction: column;
    align-items: flex-start;
  }
}
</style>
