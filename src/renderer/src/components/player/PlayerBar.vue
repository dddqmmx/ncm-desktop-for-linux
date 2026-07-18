<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { formatCurrentSongArtists, isLocalSong, usePlayerStore } from '@renderer/stores/playerStore'
import { ref, computed, onMounted } from 'vue'
// 1. 必须导入新设计的播放列表组件
import PlaylistOverlay from './PlaylistOverlay.vue'
import SongCover from '@renderer/components/media/SongCover.vue'
import AddToPlaylistModal from '@renderer/components/overlays/AddToPlaylistModal.vue'
import { useFavoriteStore } from '@renderer/stores/favoriteStore'

const playerStore = usePlayerStore()
const favoriteStore = useFavoriteStore()
const isAddToPlaylistVisible = ref(false)

// --- UI 显示绑定 ---
const displayTrack = computed(() => ({
  title: playerStore.currentSong?.name || '未在播放',
  artist: formatCurrentSongArtists(playerStore.currentSong?.artists)
}))
const isCurrentSongLocal = computed(() => isLocalSong(playerStore.currentSong))
const hasCurrentSongCover = computed(() => Boolean(playerStore.currentSong?.cover?.trim()))

// --- 进度条逻辑 ---
const isDragging = ref(false)

const beginSeek = (): void => {
  if (!isDragging.value) isDragging.value = true
  if (!playerStore.isSeeking) {
    console.log('[BAR] beginSeek: isSeeking -> true')
    playerStore.isSeeking = true
  }
}

const endSeek = (): void => {
  console.log('[BAR] endSeek: scheduling isSeeking=false in 50ms')
  setTimeout(() => {
    console.log('[BAR] endSeek TIMER: isSeeking -> false, isLoading=', playerStore.isLoading)
    isDragging.value = false
    playerStore.isSeeking = false
  }, 50)
}

const handleInput = (e: Event): void => {
  beginSeek()
  const targetTime = Number((e.target as HTMLInputElement).value)
  playerStore.currentTime = targetTime
}

const handleSeek = async (e: Event): Promise<void> => {
  beginSeek()
  const targetTime = Number((e.target as HTMLInputElement).value)
  console.log(`[BAR] handleSeek: target=${targetTime}ms`)
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

const toggleCurrentFavorite = (): void => {
  if (!playerStore.currentSong || isCurrentSongLocal.value) return
  void favoriteStore.toggleFavorite(playerStore.currentSong)
}

const openAddToPlaylist = (): void => {
  if (!playerStore.currentSong || isCurrentSongLocal.value) return
  isAddToPlaylistVisible.value = true
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
            <div
              v-if="isCurrentSongLocal && !hasCurrentSongCover"
              class="local-cover-icon"
              aria-hidden="true"
            >
              <AppIcon name="music" :size="24" />
            </div>
            <SongCover v-else :id="playerStore.currentSong?.cover" size="100y100" />
          </div>
          <div class="track-metadata">
            <div class="track-title">{{ displayTrack.title }}</div>
            <div class="track-artist">{{ displayTrack.artist }}</div>
          </div>
        </div>

        <!-- 2. 中间：核心控制 + 进度条 -->
        <div class="section-center">
          <div class="playback-controls">
            <button
              class="icon-btn sm shuffle favorite-current-btn"
              :class="{ active: favoriteStore.isFavorite(playerStore.currentSongId) }"
              :disabled="!playerStore.currentSong || isCurrentSongLocal"
              :title="
                isCurrentSongLocal
                  ? '本地音乐'
                  : favoriteStore.isFavorite(playerStore.currentSongId)
                    ? '取消喜欢'
                    : '喜欢'
              "
              @click="toggleCurrentFavorite"
            >
              <AppIcon
                :name="favoriteStore.isFavorite(playerStore.currentSongId) ? 'heart-fill' : 'heart'"
                :size="14"
              />
            </button>

            <button class="icon-btn prev" @click="playerStore.playPrev()">
              <AppIcon name="prev" :size="22" />
            </button>

            <button
              class="play-main-btn"
              :class="{ loading: playerStore.isLoading }"
              :disabled="playerStore.isLoading"
              @click="playerStore.togglePlay()"
            >
              <div class="inner-glow" :class="{ active: playerStore.isPlaying }"></div>
              <div v-if="playerStore.isLoading" class="loading-spinner"></div>
              <AppIcon v-else :name="playerStore.isPlaying ? 'pause' : 'play'" :size="26" />
            </button>

            <button class="icon-btn next" @click="playerStore.playNext()">
              <AppIcon name="next" :size="22" />
            </button>

            <button class="icon-btn sm loop" @click="playerStore.togglePlayMode()">
              <AppIcon
                :name="
                  playerStore.playMode === 'random'
                    ? 'shuffle'
                    : playerStore.playMode === 'single'
                      ? 'single'
                      : 'loop'
                "
                :size="16"
              />
            </button>
          </div>

          <div class="progress-area">
            <div class="progress-track">
              <div
                class="buffered-bar"
                :style="{
                  width: Math.max(playerStore.bufferedPercent, playerStore.progressPercent) + '%'
                }"
              ></div>
              <div class="progress-bar" :style="{ width: playerStore.progressPercent + '%' }"></div>
              <input
                type="range"
                min="0"
                :max="playerStore.duration"
                step="1"
                :value="playerStore.currentTime"
                class="hidden-range"
                @input="handleInput"
                @change="handleSeek"
              />
            </div>
          </div>
        </div>

        <!-- 3. 右侧：功能按钮 -->
        <div class="section-right">
          <button
            class="icon-btn list-btn"
            :class="{ active: isPlaylistVisible }"
            @click="togglePlaylist"
          >
            <AppIcon name="playlist" :size="20" />
          </button>
          <button
            class="icon-btn more-btn"
            :disabled="!playerStore.currentSong || isCurrentSongLocal"
            :title="isCurrentSongLocal ? '本地音乐无法添加到网易云歌单' : '添加到歌单'"
            @click="openAddToPlaylist"
          >
            <AppIcon name="more-fill" :size="20" />
          </button>
        </div>
      </div>
    </div>

    <AddToPlaylistModal
      v-if="isAddToPlaylistVisible && playerStore.currentSong && !isCurrentSongLocal"
      :song-id="playerStore.currentSong.id"
      :song-name="playerStore.currentSong.name"
      @close="isAddToPlaylistVisible = false"
    />
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
  background: var(--sys-surface);
  backdrop-filter: var(--sys-glass-blur);
  -webkit-backdrop-filter: var(--sys-glass-blur);
  border-radius: 36px;
  border: 0.5px solid var(--sys-border);
  box-shadow: var(--sys-shadow-elevated);
  padding: 0 24px;
  display: flex;
  align-items: center;
}

.favorite-current-btn.active {
  color: var(--theme-color-strong);
}

.favorite-current-btn.active svg {
  fill: currentColor;
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
  -webkit-app-region: no-drag;
}

.local-cover-icon {
  width: 100%;
  height: 100%;
  display: grid;
  place-items: center;
  background: var(--sys-control);
  color: var(--theme-color-strong);
}

.local-cover-icon svg {
  width: 24px;
  height: 24px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.track-metadata {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  -webkit-app-region: no-drag;
}

.track-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--sys-text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.track-artist {
  font-size: 11px;
  color: var(--sys-text-tertiary);
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
  background: var(--theme-color);
  color: #fff;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  position: relative;
  box-shadow: 0 4px 10px rgba(0, 0, 0, 0.05);
  -webkit-app-region: no-drag;
}

.play-main-btn:disabled {
  cursor: wait;
}

.play-main-btn.loading svg {
  opacity: 0;
}

.loading-spinner {
  position: absolute;
  width: 20px;
  height: 20px;
  border: 2px solid rgba(255, 255, 255, 0.34);
  border-top-color: var(--sys-on-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
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
  background: var(--sys-control);
  border-radius: 2px;
  position: relative;
}

.progress-bar {
  position: relative;
  height: 100%;
  background: var(--theme-color);
  border-radius: 2px;
  z-index: 1;
}

.buffered-bar {
  position: absolute;
  inset: 0 auto 0 0;
  height: 100%;
  background: var(--theme-color-muted);
  border-radius: 2px;
  transition: width 0.25s ease;
}

.hidden-range {
  position: absolute;
  top: -10px;
  left: 0;
  width: 100%;
  height: 24px;
  opacity: 0;
  cursor: pointer;
  z-index: 3;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.section-right {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.icon-btn {
  background: none;
  border: none;
  color: var(--sys-text-secondary);
  cursor: pointer;
  padding: 8px;
  border-radius: 50%;
  transition: all 0.2s;
  -webkit-app-region: no-drag;
}

.icon-btn:hover {
  background: var(--sys-control-hover);
  color: var(--sys-text);
}

.icon-btn.active {
  color: var(--theme-color-strong);
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
