<script setup lang="ts">
import { useRoute } from 'vue-router'
import { ref, computed, watch } from 'vue'
import { PlaylistDetail, Track } from '@renderer/types/playlistDetail'
import { CurrentSong, usePlayerStore } from '@renderer/stores/playerStore'

const route = useRoute()

// --- 响应式数据 ---
const detail = ref<PlaylistDetail | null>(null)
const loading = ref(true)
const activeSongId = ref<number | null>(null)

// --- 格式化工具 ---
const formatDuration = (ms: number) => {
  const totalSeconds = Math.floor(ms / 1000)
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}:${seconds.toString().padStart(2, '0')}`
}

const formatCount = (num: number) => {
  if (num >= 100000000) return (num / 100000000).toFixed(1) + '亿'
  if (num >= 10000) return (num / 10000).toFixed(1) + '万'
  return num.toString()
}

const formatDate = (timestamp: number) => {
  const date = new Date(timestamp)
  return `${date.getFullYear()}-${(date.getMonth() + 1).toString().padStart(2, '0')}-${date.getDate().toString().padStart(2, '0')}`
}


// --- 数据获取 ---
const fetchPlaylistDetail = async (playlistId) => {
  try {
    loading.value = true
    const res = await window.api.playlist_detail({ id: playlistId }) as { body?: PlaylistDetail }
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
const mapTrackToCurrentSong = (track: any): CurrentSong => ({
  id: track.id,
  name: track.name,
  artist: track.ar.map((a: any) => a.name).join(', '),
  cover: track.al.picUrl,
  duration: track.dt
})


// --- 处理“播放全部”按钮点击 ---
const handlePlayAll = () => {
  if (!tracks.value.length) return

  // 转换整个列表
  const songList = tracks.value.map(mapTrackToCurrentSong)

  // 调用 store 里的 playAll
  playerStore.playAll(songList)

  // 更新当前活跃 ID（可选，用于 UI 高亮）
  activeSongId.value = songList[0].id
}


const handlePlaySong = (song: Track) => {
  activeSongId.value = song.id
  playerStore.playMusic(song.id)
}

watch(() => route.params.id, (playlistId) => {
  if (!playlistId) return
  fetchPlaylistDetail(playlistId)
}, { immediate: true })

const searchQuery = ref('')

const playlist = computed(() => detail.value?.playlist)
const tracks = computed(() => detail.value?.playlist.tracks || [])


const filteredTracks = computed(() => {
  if (!searchQuery.value.trim()) {
    return tracks.value
  }
  const query = searchQuery.value.toLowerCase()
  return tracks.value.filter(track => {
    return (
      track.name.toLowerCase().includes(query) || // 搜索歌名
      track.al.name.toLowerCase().includes(query) || // 搜索专辑名
      track.ar.some(artist => artist.name.toLowerCase().includes(query)) // 搜索歌手名
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

    <div v-else-if="playlist" class="playlist-container">
      <!-- 歌单头部 -->
      <header class="playlist-header">
        <div class="cover-wrapper">
          <img :src="playlist.coverImgUrl + '?param=400y400'" :alt="playlist.name" class="playlist-cover">
        </div>
        <div class="playlist-details">
          <div class="playlist-type-tag">PLAYLIST</div>
          <h1 class="playlist-title">{{ playlist.name }}</h1>

          <div class="creator-info">
            <img :src="playlist.creator.avatarUrl + '?param=50y50'" class="creator-avatar">
            <span class="creator-name">{{ playlist.creator.nickname }}</span>
            <span class="create-time">{{ formatDate(playlist.createTime) }} 创建</span>
          </div>

          <p class="playlist-description" v-if="playlist.description">
            {{ playlist.description }}
          </p>

          <div class="playlist-meta">
            <span>{{ playlist.trackCount }} 首歌曲</span>
            <span class="dot"></span>
            <span>{{ formatCount(playlist.playCount) }} 次播放</span>
          </div>

          <div class="action-bar">
            <button class="play-main-btn" @click="handlePlayAll()">
              <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18"><path d="M8 5v14l11-7z" /></svg>
              播放全部
            </button>
            <div class="search-box-container">
              <svg class="search-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>
              <input
                v-model="searchQuery"
                type="text"
                placeholder="在歌单内搜索..."
                class="search-input"
              >
              <button v-if="searchQuery" class="clear-btn" @click="searchQuery = ''">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>
              </button>
            </div>
            <button class="secondary-btn">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.29 1.5 4.04 3 5.5l7 7 7-7z"></path>
              </svg>
              {{ formatCount(playlist.subscribedCount) }}
            </button>
            <button class="icon-btn">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/></svg>
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
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
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
              <svg class="play-icon" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
            </div>

            <div class="col-title">
              <img :src="track.al.picUrl + '?param=80y80'" class="mini-cover" loading="lazy">
              <div class="song-info">
                <span class="song-name">{{ track.name }}</span>
                <span class="song-artist">
                  {{ track.ar.map(a => a.name).join(' / ') }}
                </span>
              </div>
            </div>

            <div class="col-album">
              <span class="album-name">{{ track.al.name }}</span>
            </div>

            <div class="col-time">
              <span class="duration-text">{{ formatDuration(track.dt) }}</span>
              <button class="row-more">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/></svg>
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
5
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
  background: rgba(0,0,0,0.1);
  border-radius: 10px;
}


.playlist-container {
  padding: 32px 40px;
  max-width: 1200px;
  margin: 0 auto;
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

.playlist-cover {
  width: 100%;
  height: 100%;
  object-fit: cover;
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
  color: rgba(0,0,0,0.5);
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

.dot { width: 3px; height: 3px; background: #ccc; border-radius: 50%; }

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
  background: rgba(0,0,0,0.04);
  border: 1px solid rgba(0,0,0,0.05);
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
  border: 1px solid rgba(0,0,0,0.1);
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
  border-bottom: 1px solid rgba(0,0,0,0.06);
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
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.5); /* 内部顶部微光，增加立体感 */
}

.track-row.is-active .song-name {
    color: #111;
}

.col-index { width: 40px; color: #ccc; }
.col-title { flex: 3; display: flex; align-items: center; gap: 14px; min-width: 0; }
.col-album { flex: 2; font-size: 13px; color: #888; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.col-time { width: 80px; display: flex; align-items: center; justify-content: flex-end; color: #aaa; font-size: 12px; }

.play-icon { display: none; width: 14px; height: 14px; color: #111; }
.track-row:hover .index-num { display: none; }
.track-row:hover .play-icon { display: block; }

.mini-cover {
  width: 40px;
  height: 40px;
  border-radius: 6px;
  background: rgba(0,0,0,0.05);
}

.song-info { display: flex; flex-direction: column; min-width: 0; }
.song-name { font-size: 14px; font-weight: 500; color: #222; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.song-artist { font-size: 12px; color: #999; }

.row-more {
  display: none;
  background: none;
  border: none;
  color: #ccc;
  cursor: pointer;
}
.track-row:hover .row-more { display: block; }

.loading-state {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 300px;
}

.spinner {
  width: 30px;
  height: 30px;
  border: 3px solid rgba(0,0,0,0.1);
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
  to { transform: rotate(360deg); }
}

.spacer-bottom { height: 80px; }

@media (max-width: 900px) {
  .col-album { display: none; }
  .playlist-header { flex-direction: column; align-items: flex-start; }
}
</style>
