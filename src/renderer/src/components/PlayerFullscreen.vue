<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import { computed, ref, watch } from 'vue'
import ColorThief from 'colorthief'
import LyricPanel from './LyricPanel.vue'

const playerStore = usePlayerStore()
const isDragging = ref(false)
const imgRef = ref<HTMLImageElement | null>(null)

// 响应式主题变量
const theme = ref({
  primary: '#4a3f81',
  secondary: '#1d1b31',
  text: '#ffffff',
  isDark: true // 是否是深色背景
})

// 格式化时间
const formatTime = (ms: number) => {
  if (!ms) return '0:00'
  const s = Math.floor(ms / 1000)
  const min = Math.floor(s / 60)
  const sec = Math.floor(s % 60)
  return `${min}:${sec < 10 ? '0' : ''}${sec}`
}

const progressPercent = computed(() => playerStore.progressPercent)

// 处理进度跳转
const handleSeek = async (e: Event) => {
  const val = Number((e.target as HTMLInputElement).value)
  const targetTime = (val / 100) * (playerStore.currentSong?.duration ?? 0)
  await playerStore.seek(targetTime)
  setTimeout(() => {
    isDragging.value = false
  }, 500)
}

const handleInput = (e: Event) => {
  isDragging.value = true
  const val = Number((e.target as HTMLInputElement).value)
  playerStore.currentTime = (val / 100) * (playerStore.currentSong?.duration ?? 0)
}

/**
 * 核心逻辑：提取颜色并计算对比度
 */
const updateTheme = () => {
  if (!imgRef.value) return

  const colorThief = new ColorThief()
  try {
    // 获取调色板（取前 3 个颜色确保质量）
    const palette = colorThief.getPalette(imgRef.value, 3)
    if (!palette) return

    const [r, g, b] = palette[0] // 主色
    const [r2, g2, b2] = palette[1] // 辅色

    // 计算亮度 (Relative Luminance)
    // 算法：0.299R + 0.587G + 0.114B
    const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255

    theme.value = {
      primary: `rgb(${r}, ${g}, ${b})`,
      secondary: `rgb(${r2}, ${g2}, ${b2})`,
      // 如果亮度大于 0.6，认为是浅色背景，使用黑色文字；否则使用白色文字
      text: luminance > 0.6 ? '#000000' : '#ffffff',
      isDark: luminance <= 0.6
    }
  } catch (err) {
    console.error("提取颜色失败", err)
  }
}

// 监听歌曲变化，当图片加载完成后提取颜色
const onImageLoad = () => {
  updateTheme()
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
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
            <path d="M6 9l6 6 6-6"/>
          </svg>
        </button>
        <div class="playing-status">Now Playing</div>
        <div class="empty-space"></div>
      </header>

      <main class="main-layout">
        <!-- 左侧：封面与控制器 -->
        <section class="visual-panel">
          <div class="cover-wrapper">
            <img
              ref="imgRef"
              :src="playerStore.currentSong?.cover"
              class="main-cover"
              :class="{ 'playing': playerStore.isPlaying }"
              crossorigin="anonymous"
              @load="onImageLoad"
            >
          </div>
          <div class="track-meta">
            <h1 class="track-title">{{ playerStore.currentSong?.name || '未知歌曲' }}</h1>
            <p class="track-artist">{{ playerStore.currentSong?.artist || '未知艺术家' }}</p>
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
                  max="100"
                  :value="progressPercent"
                  @input="handleInput"
                  @change="handleSeek"
                />
                <div class="progress-bar-fill" :style="{ width: progressPercent + '%' }"/>
              </div>
              <span class="time-text">{{ formatTime(playerStore.duration) }}</span>
            </div>

            <!-- 控制按钮 -->
            <div class="btns-row">
              <button class="icon-btn secondary" @click="playerStore.playPrev()">
                <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/></svg>
              </button>

              <button class="play-main-btn" @click="playerStore.togglePlay">
                <svg v-if="playerStore.isPlaying" width="32" height="32" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/>
                </svg>
                <svg v-else width="32" height="32" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M8 5v14l11-7z" />
                </svg>
              </button>

              <button class="icon-btn secondary" @click="playerStore.playNext()">
                <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/></svg>
              </button>
            </div>
          </div>
        </section>

        <!-- 右侧：歌词面板 -->
        <LyricPanel
          :song-id="playerStore.currentSong?.id"
          :current-time="playerStore.currentTime"
          :is-dark="theme.isDark"
        />
      </main>
    </div>
  </div>
</template>

<style scoped>
/* 1. 基础布局与背景 - 使用 CSS 变量 */
.player-fullscreen {
  position: fixed;
  inset: 0;
  z-index: 1000;
  font-family: 'Inter', -apple-system, sans-serif;
  color: var(--text-color); /* 动态文本颜色 */
  transition: color 0.8s ease;
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
  /* 这里的阴影也根据背景亮度微调 */
  background: radial-gradient(circle at 30% 50%, transparent 0%, rgba(0,0,0,0.2) 100%);
  z-index: -1;
}

.player-content {
  height: 100%;
  display: flex;
  flex-direction: column;
  padding: 40px 60px;
  backdrop-filter: blur(40px);
}

/* 2. 顶部栏 */
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
}

.playing-status { font-weight: 600; font-size: 14px; text-transform: uppercase; letter-spacing: 2px; opacity: 0.8; }

/* 3. 主布局结构 */
.main-layout {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 1fr 1fr;
  align-items: stretch;
  overflow: hidden;
}

/* 4. 左侧封面区 */
.visual-panel {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
}

.cover-wrapper {
  width: 100%;
  max-width: 450px;
  aspect-ratio: 1;
}

.main-cover {
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: 24px;
  box-shadow: 0 30px 60px rgba(0,0,0,0.3);
  transition: transform 0.8s cubic-bezier(0.2, 0, 0.2, 1);
}

.main-cover.playing { transform: scale(1.02); }

.track-meta { margin-top: 40px; margin-bottom: 30px; }
.track-title { font-size: 38px; font-weight: 800; margin: 0 0 8px 0; }
.track-artist { font-size: 18px; opacity: 0.7; }

/* 5. 进度条与控制 */
.playback-controls { width: 100%; max-width: 450px; }
.progress-container { display: flex; align-items: center; gap: 15px; margin-bottom: 30px; }
.time-text { font-size: 12px; font-weight: 600; opacity: 0.6; min-width: 45px; }

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
}

.btns-row { display: flex; align-items: center; justify-content: center; gap: 40px; }

.play-main-btn {
  width: 72px; height: 72px; border-radius: 50%; border: none;
  background: var(--btn-bg);
  color: var(--btn-text);
  cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: transform 0.2s, background 0.5s;
}

.play-main-btn:hover { transform: scale(1.1); }
.icon-btn { background: none; border: none; color: var(--text-color); opacity: 0.8; cursor: pointer; }
.icon-btn:hover { opacity: 1; }

@media (max-width: 1000px) {
  .main-layout { grid-template-columns: 1fr; gap: 40px; }
  .player-content { padding: 20px; }
  .visual-panel { order: 1; }
  .track-title { font-size: 28px; }
}
</style>
