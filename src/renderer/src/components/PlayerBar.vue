<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import { ref, computed, onMounted } from 'vue'
// 1. 必须导入新设计的播放列表组件
import PlaylistOverlay from './PlaylistOverlay.vue'

const playerStore = usePlayerStore()

// --- UI 显示绑定 ---
const displayTrack = computed(() => ({
  title: playerStore.currentSong?.name || '未在播放',
  artist: playerStore.currentSong?.artist || '无',
  cover: playerStore.currentSong?.cover || 'https://placehold.co/100x100/444444/fff?text=None'
}))

// --- 进度条逻辑 ---
const isDragging = ref(false)

const beginSeek = (): void => {
  if (!isDragging.value) isDragging.value = true
  if (!playerStore.isSeeking) playerStore.isSeeking = true
}

const endSeek = (): void => {
  setTimeout(() => {
    isDragging.value = false
    playerStore.isSeeking = false
  }, 500)
}

const handleInput = (e: Event): void => {
  beginSeek()
  const val = Number((e.target as HTMLInputElement).value)
  playerStore.currentTime = (val / 100) * playerStore.duration
}

const handleSeek = async (e: Event): Promise<void> => {
  beginSeek()
  const val = Number((e.target as HTMLInputElement).value)
  const targetTime = (val / 100) * playerStore.duration
  try {
    await playerStore.seek(targetTime)
  } finally {
    endSeek()
  }
}

// 播放列表状态
const isPlaylistVisible = ref(false)
const togglePlaylist = (): void => {
  isPlaylistVisible.value = !isPlaylistVisible.value
}

onMounted(() => {
  playerStore.initFromStorage()
})
</script>

<template>
  <div class="player-container-position">
    <!-- 2. 播放列表悬浮窗：放在播放条上方 -->
    <Transition name="slide-fade">
      <PlaylistOverlay v-if="isPlaylistVisible" class="playlist-popup" />
    </Transition>

    <!-- iOS 26 Liquid Glass 容器 -->
    <div class="player-bar glass-morphism">
      <div class="player-layout">
        <!-- 1. 左侧：歌曲信息 -->
        <div class="section-left" @click="playerStore.toggleFullScreen">
          <div class="cover-wrapper">
            <img :src="displayTrack.cover" class="track-cover">
          </div>
          <div class="track-metadata">
            <div class="track-title">{{ displayTrack.title }}</div>
            <div class="track-artist">{{ displayTrack.artist }}</div>
          </div>
        </div>

        <!-- 2. 中间：核心控制 + 进度条 -->
        <div class="section-center">
          <div class="playback-controls">
            <button class="icon-btn sm shuffle">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                stroke-linecap="round" stroke-linejoin="round">
                <path
                  d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" />
              </svg>
            </button>

            <button class="icon-btn prev" @click="playerStore.playPrev()">
              <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
                <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z" />
              </svg>
            </button>

            <button class="play-main-btn" @click="playerStore.togglePlay()">
              <div class="inner-glow" :class="{ active: playerStore.isPlaying }"></div>
              <svg width="26" height="26" viewBox="0 0 24 24" fill="currentColor">
                <path v-if="playerStore.isPlaying" d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
                <path v-else d="M8 5v14l11-7z" />
              </svg>
            </button>

            <button class="icon-btn next" @click="playerStore.playNext()">
              <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
                <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" />
              </svg>
            </button>

            <button class="icon-btn sm loop" @click="playerStore.togglePlayMode()">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                stroke-linecap="round" stroke-linejoin="round">

                <g class="icon-loop" v-if="playerStore.playMode == 'loop'">
                  <path d="M17 2l4 4-4 4" />
                  <path d="M3 11V9a4 4 0 0 1 4-4h14" />
                  <path d="M7 22l-4-4 4-4" />
                  <path d="M21 13v2a4 4 0 0 1-4 4H3" />
                </g>

                <g class="icon-random" v-if="playerStore.playMode == 'random'">
                  <path d="M16 3h5v5" />
                  <path d="M4 20L21 3" />
                  <path d="M21 16v5h-5" />
                  <path d="M15 15l6 6" />
                  <path d="M4 4l5 5" />
                </g>

                <g class="icon-single" v-if="playerStore.playMode == 'single'">
                  <path d="M17 2l4 4-4 4" />
                  <path d="M3 11V9a4 4 0 0 1 4-4h14" />
                  <path d="M7 22l-4-4 4-4" />
                  <path d="M21 13v2a4 4 0 0 1-4 4H3" />
                  <path d="M11 10h1v4" stroke-width="2" />
                </g>

              </svg>
            </button>
          </div>

          <div class="progress-area">
            <div class="progress-track">
              <div class="progress-bar" :style="{ width: playerStore.progressPercent + '%' }"></div>
              <input type="range" min="0" max="100" step="0.1" :value="playerStore.progressPercent" @input="handleInput"
                @change="handleSeek" class="hidden-range">
            </div>
          </div>
        </div>

        <!-- 3. 右侧：功能按钮 -->
        <div class="section-right">
          <button class="icon-btn list-btn" :class="{ active: isPlaylistVisible }" @click="togglePlaylist">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
              stroke-linecap="round">
              <line x1="8" y1="6" x2="21" y2="6" />
              <line x1="8" y1="12" x2="21" y2="12" />
              <line x1="8" y1="18" x2="21" y2="18" />
              <circle cx="3" cy="6" r="1" />
              <circle cx="3" cy="12" r="1" />
              <circle cx="3" cy="18" r="1" />
            </svg>
          </button>
          <button class="icon-btn more-btn">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <circle cx="5" cy="12" r="2" />
              <circle cx="12" cy="12" r="2" />
              <circle cx="19" cy="12" r="2" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 3. 合并后的容器样式 */
.player-container-position {
  position: absolute;
  bottom: 24px;
  left: 0;
  width: 100%;
  display: flex;
  flex-direction: column;
  /* 垂直排列列表和播放条 */
  align-items: center;
  /* 水平居中 */
  z-index: 100;
  pointer-events: none;
  /* 允许点击到底层内容 */
}

/* 4. 播放列表弹出位置 */
.playlist-popup {
  pointer-events: auto;
  /* 恢复点击 */
  width: 360px;
  max-width: calc(100vw - 40px);
  margin-bottom: 12px;
  /* 与播放器条的间距 */
  /* 如果想让列表对齐播放条右侧，可以放开下面两行 */
  /* align-self: flex-end;
  margin-right: calc(50% - 410px); (基于播放条 max-width 820 的一半) */
}

/* iOS 26 玻璃质感播放条 */
.glass-morphism {
  pointer-events: auto;
  /* 恢复点击 */
  width: calc(100% - 40px);
  max-width: 820px;
  height: 96px;
  background: rgba(255, 255, 255, 0.2);
  backdrop-filter: blur(50px) saturate(200%);
  -webkit-backdrop-filter: blur(50px) saturate(200%);
  border-radius: 36px;
  border: 0.5px solid rgba(255, 255, 255, 0.4);
  box-shadow: 0 20px 40px rgba(0, 0, 0, 0.1);
  padding: 0 24px;
  display: flex;
  align-items: center;
}

/* 进场和退场动画 */
.slide-fade-enter-active {
  transition: all 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}

.slide-fade-leave-active {
  transition: all 0.3s cubic-bezier(0.7, 0, 0.84, 0);
}

.slide-fade-enter-from {
  opacity: 0;
  transform: translateY(20px) scale(0.95);
  filter: blur(10px);
}

.slide-fade-leave-to {
  opacity: 0;
  transform: translateY(10px) scale(0.98);
}

/* 布局结构 */
.player-layout {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}

.section-left,
.section-right {
  flex: 1;
  min-width: 150px;
}

.section-left {
  display: flex;
  align-items: center;
  gap: 12px;
  cursor: pointer;
}

.cover-wrapper {
  width: 48px;
  height: 48px;
  border-radius: 10px;
  flex-shrink: 0;
  overflow: hidden;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}

.track-cover {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.track-metadata {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.track-title {
  font-size: 14px;
  font-weight: 700;
  color: #111;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.track-artist {
  font-size: 11px;
  color: rgba(0, 0, 0, 0.5);
}

.section-center {
  flex: 2;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  max-width: 400px;
}

.playback-controls {
  display: flex;
  align-items: center;
  gap: 16px;
}

.play-main-btn {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: white;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  position: relative;
  box-shadow: 0 4px 10px rgba(0, 0, 0, 0.05);
}

.inner-glow {
  position: absolute;
  width: 100%;
  height: 100%;
  filter: blur(8px);
  border-radius: 50%;
  opacity: 0;
  transition: opacity 0.3s;
}

.inner-glow.active {
  opacity: 0.15;
}

.progress-area {
  width: 100%;
  padding: 4px 0;
}

.progress-track {
  width: 100%;
  height: 4px;
  background: rgba(0, 0, 0, 0.05);
  border-radius: 2px;
  position: relative;
}

.progress-bar {
  height: 100%;
  background: #111;
  border-radius: 2px;
}

.hidden-range {
  position: absolute;
  top: -10px;
  left: 0;
  width: 100%;
  height: 24px;
  opacity: 0;
  cursor: pointer;
}

.section-right {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.icon-btn {
  background: none;
  border: none;
  color: rgba(0, 0, 0, 0.6);
  cursor: pointer;
  padding: 8px;
  border-radius: 50%;
  transition: all 0.2s;
}

.icon-btn:hover {
  background: rgba(0, 0, 0, 0.05);
  color: #000;
}

.icon-btn.active {
  background: rgba(0, 0, 0, 0.1);
  color: #000;
}

.icon-btn.sm {
  opacity: 0.4;
}

@media (max-width: 650px) {

  .section-left,
  .section-right {
    min-width: auto;
  }

  .track-metadata,
  .sm {
    display: none;
  }

  .playlist-popup {
    width: calc(100vw - 40px);
  }
}
</style>
