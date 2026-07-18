<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { formatCurrentSongArtists, isLocalSong, usePlayerStore } from '@renderer/stores/playerStore'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import LyricPanel from './LyricPanel.vue'
import { colord } from 'colord'
import { extractColors } from 'extract-colors'
import { useResolvedMediaUrl } from '@renderer/composables/useResolvedMediaUrl'
import SongCover from '@renderer/components/media/SongCover.vue'

const playerStore = usePlayerStore()
const router = useRouter()
const isDragging = ref(false)
const imgRef = ref<HTMLImageElement | null>(null)
const resolvedCover = useResolvedMediaUrl(() => playerStore.currentSong?.cover)

// 响应式主题变量
const theme = ref({
  primary: 'var(--theme-color)',
  secondary: 'var(--theme-color-strong)',
  text: 'var(--sys-on-accent)',
  isDark: true // 是否是深色背景
})

// 格式化时间
const formatTime = (ms: number, showMillis = false): string => {
  if (!Number.isFinite(ms) || ms <= 0) return showMillis ? '0:00.000' : '0:00'
  const totalMs = Math.round(ms)
  const min = Math.floor(totalMs / 60_000)
  const sec = Math.floor((totalMs % 60_000) / 1000)
  const baseTime = `${min}:${sec.toString().padStart(2, '0')}`
  if (!showMillis) return baseTime
  const millis = totalMs % 1000
  return `${baseTime}.${millis.toString().padStart(3, '0')}`
}

const progressPercent = computed(() => playerStore.progressPercent)
const currentArtists = computed(() => playerStore.currentSong?.artists ?? [])
const isCurrentSongLocal = computed(() => isLocalSong(playerStore.currentSong))
const hasCurrentSongCover = computed(() => Boolean(playerStore.currentSong?.cover?.trim()))

const beginSeek = (): void => {
  if (!isDragging.value) isDragging.value = true
  if (!playerStore.isSeeking) {
    console.log('[FULLSCREEN] beginSeek: isSeeking -> true')
    playerStore.isSeeking = true
  }
}

const endSeek = (): void => {
  console.log('[FULLSCREEN] endSeek: scheduling isSeeking=false in 50ms')
  setTimeout(() => {
    console.log('[FULLSCREEN] endSeek TIMER: isSeeking -> false, isLoading=', playerStore.isLoading)
    isDragging.value = false
    playerStore.isSeeking = false
  }, 50)
}

// 处理进度跳转
const handleSeek = async (e: Event): Promise<void> => {
  beginSeek()
  const targetTime = Number((e.target as HTMLInputElement).value)
  console.log(`[FULLSCREEN] handleSeek: target=${targetTime}ms`)
  try {
    await playerStore.seek(targetTime)
  } finally {
    endSeek()
  }
}

const handleInput = (e: Event): void => {
  beginSeek()
  const targetTime = Number((e.target as HTMLInputElement).value)
  playerStore.currentTime = targetTime
}

const goToArtist = async (artistId: number): Promise<void> => {
  if (!artistId) return

  playerStore.isFullScreen = false
  await router.push({
    name: 'artist',
    params: { id: artistId }
  })
}

/**
 * 核心逻辑：提取颜色并计算对比度
 */
const updateTheme = async (): Promise<void> => {
  // 必须确保图片已加载且有地址
  if (!imgRef.value || !resolvedCover.value) return

  try {
    // extractColors 支持传入图片 URL
    // 它会自动处理 Canvas 绘制和像素提取
    const colors = await extractColors(imgRef.value.src, {
      crossOrigin: 'anonymous',
      pixels: 30000 // 采样像素，越高越准但越慢，30000是平衡点
    })

    if (!colors || colors.length === 0) return

    // 1. 获取主色和辅助色 (extractColors 已按面积占比排序)
    const primaryColor = colors[0]
    const secondaryColor = colors[1] || colors[0]

    // 2. 使用 colord 处理颜色数学
    const pColor = colord(primaryColor.hex)
    const isDark = pColor.isDark() // 自动计算亮度并判断是否为深色

    theme.value = {
      primary: primaryColor.hex,
      secondary: secondaryColor.hex,
      // 智能文本色：如果是浅色背景则黑字，反之白字
      text: isDark ? '#ffffff' : '#000000',
      isDark: isDark
    }
  } catch (err) {
    console.error('提取颜色失败', err)
    // 失败时回退到默认主题
    theme.value = {
      primary: 'var(--theme-color)',
      secondary: 'var(--theme-color-strong)',
      text: 'var(--sys-on-accent)',
      isDark: true
    }
  }
}

// 当图片加载完成后触发
const onImageLoad = (): void => {
  void updateTheme()
}
</script>

<template>
  <div
    class="player-fullscreen"
    :style="{
      '--bg-primary': theme.primary,
      '--bg-secondary': theme.secondary,
      '--text-color': theme.text,
      '--contrast-color': theme.isDark ? 'rgba(255,255,255,0.15)' : 'rgba(0,0,0,0.1)',
      '--btn-bg': theme.text,
      '--btn-text': theme.isDark ? '#000' : '#fff'
    }"
  >
    <!-- 动态背景层 -->
    <div class="dynamic-background"></div>
    <div class="vignette"></div>

    <div class="player-content">
      <!-- 顶部栏 -->
      <header class="top-bar">
        <button class="back-btn" @click="playerStore.toggleFullScreen">
          <AppIcon name="chevron-down" :size="24" />
        </button>
        <div class="empty-space"></div>
      </header>

      <main class="main-layout">
        <!-- 左侧：封面与控制器 -->
        <section class="visual-panel">
          <div class="cover-wrapper">
            <div
              v-if="isCurrentSongLocal && !hasCurrentSongCover"
              class="main-cover-component local-cover-display"
              :class="{ playing: playerStore.isPlaying }"
              aria-hidden="true"
            >
              <AppIcon name="music" :size="128" />
            </div>
            <SongCover
              v-else
              :id="playerStore.currentSong?.cover"
              size="500y500"
              class="main-cover-component"
              :class="{ playing: playerStore.isPlaying }"
            />
            <img
              v-if="hasCurrentSongCover && resolvedCover"
              ref="imgRef"
              :src="resolvedCover"
              class="main-cover-hidden"
              crossorigin="anonymous"
              @load="onImageLoad"
            />
          </div>
          <div class="track-meta">
            <h1 class="track-title">{{ playerStore.currentSong?.name || '未知歌曲' }}</h1>
            <div class="track-artists">
              <template
                v-for="(artist, index) in currentArtists"
                :key="`${artist.id}-${artist.name}-${index}`"
              >
                <button
                  type="button"
                  class="track-artist"
                  :class="{ 'track-artist-link': artist.id > 0 }"
                  @click.stop="goToArtist(artist.id)"
                >
                  {{ artist.name }}
                </button>
                <span v-if="index < currentArtists.length - 1" class="track-artist-separator">
                  /
                </span>
              </template>
              <span v-if="currentArtists.length === 0" class="track-artist">
                {{ formatCurrentSongArtists(currentArtists) }}
              </span>
            </div>
          </div>

          <div class="playback-controls">
            <!-- 进度条 -->
            <div class="progress-container">
              <span class="time-text">{{ formatTime(playerStore.currentTime) }}</span>
              <div class="progress-bar-bg">
                <input
                  class="progress-input"
                  type="range"
                  min="0"
                  :max="playerStore.duration"
                  step="1"
                  :value="playerStore.currentTime"
                  @input="handleInput"
                  @change="handleSeek"
                />
                <div
                  class="progress-buffer-fill"
                  :style="{
                    width: Math.max(playerStore.bufferedPercent, playerStore.progressPercent) + '%'
                  }"
                />
                <div class="progress-bar-fill" :style="{ width: progressPercent + '%' }" />
              </div>
              <span class="time-text">{{ formatTime(playerStore.duration) }}</span>
            </div>

            <!-- 控制按钮 -->
            <div class="btns-row">
              <button class="icon-btn secondary" @click="playerStore.playPrev()">
                <AppIcon name="prev" :size="24" />
              </button>

              <button
                class="play-main-btn"
                :class="{ loading: playerStore.isLoading }"
                :disabled="playerStore.isLoading"
                @click="playerStore.togglePlay"
              >
                <div v-if="playerStore.isLoading" class="loading-spinner"></div>
                <AppIcon v-if="playerStore.isPlaying" name="pause" :size="32" />
                <AppIcon v-else name="play" :size="32" />
              </button>

              <button class="icon-btn secondary" @click="playerStore.playNext()">
                <AppIcon name="next" :size="24" />
              </button>
            </div>
          </div>
        </section>

        <!-- 右侧：歌词面板 -->
        <LyricPanel
          v-if="!isCurrentSongLocal"
          :song-id="playerStore.currentSong?.id"
          :current-time="playerStore.currentTime"
          :is-dark="theme.isDark"
          :is-seeking="playerStore.isSeeking"
        />
        <section v-else class="local-file-panel">
          <AppIcon name="music" :size="48" />
          <strong>本地音乐</strong>
          <span>{{ playerStore.currentSong?.fileName }}</span>
        </section>
      </main>
    </div>
  </div>
</template>

<style scoped>
.player-fullscreen {
  position: fixed;
  inset: 0;
  z-index: 1000;
  font-family:
    'Inter',
    -apple-system,
    sans-serif;
  color: var(--text-color);
  transition: color 0.8s ease;
  -webkit-app-region: drag;
}

.dynamic-background {
  position: absolute;
  inset: 0;
  background: linear-gradient(135deg, var(--bg-primary) 0%, var(--bg-secondary) 100%);
  transition: background 1.5s ease;
  z-index: -2;
}

.vignette {
  position: absolute;
  inset: 0;
  background: radial-gradient(circle at 30% 50%, transparent 0%, rgba(0, 0, 0, 0.2) 100%);
  z-index: -1;
}

.player-content {
  height: 100%;
  display: flex;
  flex-direction: column;
  padding: 40px 60px;
  backdrop-filter: blur(40px);
  box-sizing: border-box;
}

/* 顶部栏 */
.top-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 40px;
}

.back-btn {
  background: var(--contrast-color);
  border: none;
  color: var(--text-color);
  width: 44px;
  height: 44px;
  border-radius: 50%;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.3s;
  -webkit-app-region: no-drag;
}
/* 主布局 */
.main-layout {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 60px;
  align-items: center;
  overflow: hidden;
}

/* 防止子项被撑开 */
.visual-panel,
.lyric-section,
.local-file-panel {
  min-width: 0;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.local-file-panel {
  align-items: center;
  justify-content: center;
  gap: 14px;
  text-align: center;
  opacity: 0.74;
}

.local-file-panel svg {
  width: 64px;
  height: 64px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.4;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.local-file-panel strong {
  font-size: 22px;
  font-weight: 700;
}

.local-file-panel span {
  max-width: 420px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
}

/* 左侧内容/封面 */
.visual-panel {
  justify-content: center;
  align-items: center;
  text-align: center;
}

.cover-wrapper {
  width: 100%;
  max-width: 450px;
  aspect-ratio: 1;
  flex-shrink: 1;
  min-height: 0;
  position: relative;
}

.main-cover-component {
  width: 100%;
  height: 100%;
  border-radius: 24px;
  box-shadow: 0 30px 60px rgba(0, 0, 0, 0.3);
  transition: transform 0.8s cubic-bezier(0.2, 0, 0.2, 1);
}

.main-cover-component.playing {
  transform: scale(1.02);
}

.local-cover-display {
  display: grid;
  place-items: center;
  background: var(--sys-control);
  color: var(--theme-color-strong);
}

.local-cover-display svg {
  width: 128px;
  height: 128px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.4;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.main-cover-hidden {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  opacity: 0;
  pointer-events: none;
  object-fit: cover;
}

.track-meta {
  margin-top: 40px;
  margin-bottom: 30px;
  width: 100%;
  max-width: 450px;
  text-align: center;
  overflow: hidden;
}

.track-title {
  font-size: 38px;
  font-weight: 800;
  margin: 0 0 8px 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  width: 100%;
  display: block;
}

.track-artists {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-wrap: wrap;
  gap: 8px;
}

.track-artist {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  max-width: 100%;
  font-family: inherit;
  font-size: 18px;
  opacity: 0.7;
  background: none;
  border: none;
  color: inherit;
  padding: 0;
  -webkit-app-region: no-drag;
}

.track-artist-separator {
  opacity: 0.5;
}

.track-artist-link {
  cursor: pointer;
}

.track-artist-link:hover {
  opacity: 1;
  text-decoration: underline;
}

/* 进度条与控制 */
.playback-controls {
  width: 100%;
  max-width: 450px;
}

.progress-container {
  display: flex;
  align-items: center;
  gap: 15px;
  margin-bottom: 30px;
}
.time-text {
  font-size: 12px;
  font-weight: 600;
  opacity: 0.6;
  min-width: 72px;
  font-variant-numeric: tabular-nums;
}

.progress-container,
.progress-input {
  -webkit-app-region: no-drag;
}

.progress-bar-bg {
  position: relative;
  flex: 1;
  height: 6px;
  background: var(--contrast-color);
  border-radius: 3px;
}

.progress-input {
  position: absolute;
  inset: 0;
  width: 100%;
  opacity: 0;
  cursor: pointer;
  z-index: 2;
}

.progress-bar-fill {
  position: absolute;
  height: 100%;
  background: var(--text-color);
  border-radius: 3px;
  box-shadow: 0 0 10px var(--contrast-color);
  z-index: 1;
}

.progress-buffer-fill {
  position: absolute;
  inset: 0 auto 0 0;
  height: 100%;
  background: var(--contrast-color);
  border-radius: 3px;
  transition: width 0.25s ease;
}

.btns-row {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 40px;
}
.btns-row button {
  -webkit-app-region: no-drag;
}

.play-main-btn {
  width: 72px;
  height: 72px;
  border-radius: 50%;
  border: none;
  background: var(--btn-bg);
  color: var(--btn-text);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    transform 0.2s,
    background 0.5s;
}

.play-main-btn:disabled {
  cursor: wait;
}

.play-main-btn.loading svg {
  opacity: 0;
}

.loading-spinner {
  position: absolute;
  width: 26px;
  height: 26px;
  border: 2px solid var(--contrast-color);
  border-top-color: var(--btn-text);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.play-main-btn:hover {
  transform: scale(1.1);
}

.icon-btn {
  background: none;
  border: none;
  color: var(--text-color);
  opacity: 0.8;
  cursor: pointer;
}
.icon-btn:hover {
  opacity: 1;
}

/* 响应式 */
@media (max-width: 1100px) {
  .main-layout {
    gap: 30px;
  }
  .player-content {
    padding: 30px;
  }
}

@media (max-width: 1000px) {
  .main-layout {
    grid-template-columns: 1fr;
    gap: 40px;
  }
  .player-content {
    padding: 20px;
  }
  .visual-panel {
    order: 1;
  }
  .track-title {
    font-size: 28px;
  }
}

@media (max-width: 900px) {
  .main-layout {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr 1fr;
  }
  .track-title {
    font-size: 28px;
  }
}
</style>
