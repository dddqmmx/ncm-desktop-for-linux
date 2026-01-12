<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import { ref, computed, onMounted } from 'vue'

const playerStore = usePlayerStore()

// --- UI 显示绑定 ---
const displayTrack = computed(() => ({
  title: playerStore.currentSong?.name || '未在播放',
  artist: playerStore.currentSong?.artist || '无',
  cover: playerStore.currentSong?.cover || 'https://placehold.co/100x100/444444/fff?text=None'
}))

// --- 进度条逻辑 ---
const isDragging = ref(false)

// 拖动中：仅修改本地展示，不触发 API
const handleInput = (e: Event) => {
  isDragging.value = true
  const val = Number((e.target as HTMLInputElement).value)
  playerStore.currentTime = (val / 100) * playerStore.duration
}

// 拖动结束：触发 Store 的 seek
const handleSeek = async (e: Event) => {
  const val = Number((e.target as HTMLInputElement).value)
  const targetTime = (val / 100) * playerStore.duration

  await playerStore.seek(targetTime)

  setTimeout(() => {
    isDragging.value = false
  }, 500)
}

// 音量控制 (示例)
const volume = ref(100)
const handleVolumeChange = (e: Event) => {
  volume.value = Number((e.target as HTMLInputElement).value)
  // window.api.set_volume(volume.value / 100)
}

onMounted(() => {
  playerStore.initFromStorage()
})
</script>

<template>
  <div class="player-container-position">
    <div class="player-bar crystal-texture">

      <div class="player-content">
        <!-- Left Controls -->
        <div class="controls-left">
          <button class="ctrl-btn shuffle">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M16 3h5v5M4 20L21 3M21 16v5h-5M15 15l-5 5M4 4l5 5"/></svg>
          </button>
          <button class="ctrl-btn prev">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/></svg>
          </button>

          <button class="ctrl-btn play-pause" @click="playerStore.togglePlay">
            <div class="play-btn-glow" :style="{ opacity: playerStore.isPlaying ? 0.5 : 0 }"></div>
            <svg width="32" height="32" viewBox="0 0 24 24" fill="currentColor" style="position:relative; z-index:2;">
              <path v-if="playerStore.isPlaying" d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/>
              <path v-else d="M8 5v14l11-7z"/>
            </svg>
          </button>

          <button class="ctrl-btn next">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/></svg>
          </button>
          <button class="ctrl-btn loop">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M17 1l4 4-4 4" /><path d="M3 11V9a4 4 0 0 1 4-4h14" /><path d="M7 23l-4-4 4-4" /><path d="M21 13v2a4 4 0 0 1-4 4H3" /></svg>
          </button>
        </div>

        <!-- Absolutely Centered Track Info -->
        <div class="track-info-centered" @click="playerStore.toggleFullScreen">
          <img :src="displayTrack.cover" class="track-cover">
          <div class="track-text">
            <div class="track-title">{{ displayTrack.title }}</div>
            <div class="track-artist">{{ displayTrack.artist }}</div>
          </div>
        </div>

        <!-- Right Volume & More -->
        <div class="controls-right">
          <div class="volume-control">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
            <input type="range" min="0" max="100" :value="volume" @input="handleVolumeChange" class="volume-slider">
          </div>
          <button class="ctrl-btn">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>
          </button>
        </div>
      </div>

      <!-- Independent Progress Bar at Bottom -->
      <div class="progress-container">
        <div class="progress-bar-wrapper">
          <div class="progress-fill" :style="{ width: playerStore.progressPercent + '%' }"></div>
          <input
            type="range"
            min="0"
            max="100"
            step="0.1"
            :value="playerStore.progressPercent"
            @input="handleInput"
            @change="handleSeek"
            class="progress-slider">
        </div>
      </div>

    </div>
  </div>
</template>

<style scoped>
.player-container-position {
  position: absolute;
  bottom: 20px;
  left: 0;
  width: 100%;
  display: flex;
  justify-content: center;
  z-index: 50;
  pointer-events: none;
}

.crystal-texture {
  pointer-events: auto;
  width: calc(100% - 40px);
  max-width: 850px;
  height: 100px; /* 固定高度确保居中对齐 */
  padding: 0 24px;
  background: rgba(255, 255, 255, 0.75);
  backdrop-filter: blur(25px) saturate(180%);
  -webkit-backdrop-filter: blur(25px) saturate(180%);
  border-radius: 32px;
  border: 1px solid rgba(255, 255, 255, 0.8);
  box-shadow: 0 20px 50px rgba(0, 0, 0, 0.1);
  position: relative;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.player-content {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  height: 100%;
}

/* --- Track Info 绝对居中 --- */
.track-info-centered {
  position: absolute;
  left: 50%;
  top: 45%; /* 稍微偏上，给底部进度条留出视觉平衡空间 */
  transform: translate(-50%, -50%);
  display: flex;
  align-items: center;
  gap: 12px;
  z-index: 10;
  pointer-events: auto; /* 防止遮挡下方按钮点击 */
  cursor: pointer;
}

.track-cover {
  width: 48px;
  height: 48px;
  border-radius: 12px;
  object-fit: cover;
  box-shadow: 0 8px 16px rgba(0,0,0,0.1);
}

.track-text {
  display: flex;
  flex-direction: column;
  max-width: 180px;
}

.track-title {
  font-size: 14px;
  font-weight: 800;
  color: #111;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.track-artist {
  font-size: 11px;
  color: #888;
  font-weight: 500;
}

/* --- 进度条 (底部定位，不影响高度) --- */
.progress-container {
  position: absolute;
  bottom: 18px; /* 固定在容器底部 */
  left: 220px;   /* 避开左侧控制按钮 */
  right: 180px;  /* 避开右侧控制按钮 */
  display: flex;
  align-items: center;
}

.progress-bar-wrapper {
  position: relative;
  width: 100%;
  height: 4px;
  background: rgba(0, 0, 0, 0.05);
  border-radius: 10px;
  overflow: hidden;
  transition: height 0.2s;
}

.progress-container:hover .progress-bar-wrapper {
  height: 6px;
}

.progress-fill {
  position: absolute;
  left: 0;
  height: 100%;
  background: linear-gradient(90deg, #333, #000);
  border-radius: 10px;
  z-index: 1;
}

.progress-slider {
  position: absolute;
  width: 100%;
  height: 100%;
  margin: 0;
  opacity: 0;
  z-index: 2;
  cursor: pointer;
}

/* --- 其他布局保持原样 --- */
.controls-left, .controls-right { display: flex; align-items: center; gap: 16px; flex: 1; pointer-events: auto; }
.controls-right { justify-content: flex-end; }

.ctrl-btn {
  background: none; border: none; padding: 0;
  color: #444; cursor: pointer; transition: all 0.2s ease;
  display: flex; align-items: center; justify-content: center; position: relative;
}
.ctrl-btn:hover { transform: scale(1.1); color: #000; }

.play-btn-glow {
  position: absolute; width: 44px; height: 44px;
  background: rgba(255, 255, 255, 1);
  filter: blur(8px); border-radius: 50%; z-index: 1;
}

.volume-control { display: flex; align-items: center; gap: 8px; color: #666; margin-right: 10px; }
.volume-slider { width: 60px; accent-color: #444; height: 3px; cursor: pointer; }

@media (max-width: 800px) {
  .track-info-centered, .volume-control { display: none; }
  .progress-container { left: 40px; right: 40px; }
}
</style>
