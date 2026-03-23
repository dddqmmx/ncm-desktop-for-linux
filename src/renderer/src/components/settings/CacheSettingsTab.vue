<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import SettingGroup from '@renderer/components/settings/SettingGroup.vue'
import { useConfigStore } from '@renderer/stores/configStore'

const configStore = useConfigStore()
const {
  cacheStats,
  isLoadingCacheStats,
  isUpdatingCacheLimit,
  isUpdatingSongCacheAheadSecs,
  isClearingCache,
  cacheError,
  cacheLimitMb,
  songCacheAheadSecs
} = storeToRefs(configStore)

const message = ref('')
const messageType = ref<'success' | 'error'>('success')
const cacheLimitDraft = ref(cacheLimitMb.value)
const songCacheAheadDraft = ref(songCacheAheadSecs.value)

watch(cacheLimitMb, (value) => {
  cacheLimitDraft.value = value
})

watch(songCacheAheadSecs, (value) => {
  songCacheAheadDraft.value = value
})

// === 计算滑块进度的百分比用于 div 宽度渲染 ===
const cacheLimitPercent = computed(() => {
  const val = Number(cacheLimitDraft.value) || 128
  return Math.max(0, Math.min(100, ((val - 128) / (8192 - 128)) * 100))
})

const songCacheAheadPercent = computed(() => {
  const val = Number(songCacheAheadDraft.value) || 10
  return Math.max(0, Math.min(100, ((val - 10) / (300 - 10)) * 100))
})
// =============================================

const formatBytes = (bytes: number): string => {
  if (bytes >= 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
  }
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`
  }
  return `${bytes} B`
}

const cacheUsageText = computed(() => {
  return `${formatBytes(cacheStats.value.totalBytes)} / ${formatBytes(cacheStats.value.maxSizeBytes)}`
})

const cacheUsagePercent = computed(() => {
  if (cacheStats.value.maxSizeBytes <= 0) {
    return 0
  }
  return Math.min(100, (cacheStats.value.totalBytes / cacheStats.value.maxSizeBytes) * 100)
})

const cacheBreakdown = computed(() =>[
  {
    label: '歌曲',
    usage: formatBytes(cacheStats.value.songBytes),
    count: `${cacheStats.value.songEntries} 首`
  },
  {
    label: '歌手 / 用户头像',
    usage: formatBytes(cacheStats.value.entityBytes),
    count: `${cacheStats.value.entityEntries} 个`
  },
  {
    label: '封面',
    usage: formatBytes(cacheStats.value.coverBytes),
    count: `${cacheStats.value.coverEntries} 个`
  },
  {
    label: '歌词',
    usage: formatBytes(cacheStats.value.lyricBytes),
    count: `${cacheStats.value.lyricEntries} 首`
  }
])

const applyCacheLimit = async (): Promise<void> => {
  const applied = await configStore.setCacheLimit(cacheLimitDraft.value)
  messageType.value = applied ? 'success' : 'error'
  message.value = applied ? '缓存上限已更新。' : '缓存上限更新失败。'
}

const applySongCacheAhead = async (): Promise<void> => {
  const applied = await configStore.setSongCacheAheadTime(songCacheAheadDraft.value)
  messageType.value = applied ? 'success' : 'error'
  message.value = applied ? '歌曲预缓存时长已更新。' : '歌曲预缓存时长更新失败。'
}

const clearCache = async (): Promise<void> => {
  const cleared = await configStore.clearCache()
  messageType.value = cleared ? 'success' : 'error'
  message.value = cleared ? '缓存已清理。' : '缓存清理失败。'
}
</script>

<template>
   <SettingGroup
      title="本地缓存"
      tip="native 缓存会维护歌曲、歌手/用户信息、封面和歌词；歌曲音频改为边播边缓存，并在超限时自动回收冷数据。"
      no-card
    >
      <div class="cache-card">
        <div class="cache-header">
          <div>
            <p class="cache-label">当前占用</p>
            <p class="cache-value">{{ cacheUsageText }}</p>
          </div>
          <button
            class="settings-inline-action-btn"
            :disabled="isClearingCache || isLoadingCacheStats"
            @click="clearCache"
          >
            {{ isClearingCache ? '清理中...' : '清理缓存' }}
          </button>
        </div>

        <div class="cache-progress">
          <div class="cache-progress-fill" :style="{ width: `${cacheUsagePercent}%` }"></div>
        </div>

        <div class="cache-limit-row">
          <label class="cache-limit-label" for="cache-limit-range">最大缓存占用</label>
          <div class="cache-limit-controls">

            <!-- === 修改为 DIV 模拟视觉 + input 隐藏交互 === -->
            <div class="custom-slider-wrapper">
              <div class="custom-slider-track">
                <div class="custom-slider-fill" :style="{ width: cacheLimitPercent + '%' }"></div>
              </div>
              <input
                id="cache-limit-range"
                v-model.number="cacheLimitDraft"
                class="custom-slider-input"
                type="range"
                min="128"
                max="8192"
                step="128"
              />
            </div>
            <!-- ======================================= -->

            <input
              v-model.number="cacheLimitDraft"
              class="cache-number-input"
              type="number"
              min="128"
              max="8192"
              step="128"
            />
            <span class="cache-unit">MB</span>
            <button
              class="settings-action-btn"
              :disabled="isUpdatingCacheLimit || isLoadingCacheStats"
              @click="applyCacheLimit"
            >
              {{ isUpdatingCacheLimit ? '应用中...' : '应用' }}
            </button>
          </div>
        </div>

        <div class="cache-limit-row">
          <label class="cache-limit-label" for="song-cache-ahead-range">歌曲预缓存时长</label>
          <div class="cache-limit-controls">

            <!-- === 修改为 DIV 模拟视觉 + input 隐藏交互 === -->
            <div class="custom-slider-wrapper">
              <div class="custom-slider-track">
                <div class="custom-slider-fill" :style="{ width: songCacheAheadPercent + '%' }"></div>
              </div>
              <input
                id="song-cache-ahead-range"
                v-model.number="songCacheAheadDraft"
                class="custom-slider-input"
                type="range"
                min="10"
                max="300"
                step="10"
              />
            </div>
            <!-- ======================================= -->

            <input
              v-model.number="songCacheAheadDraft"
              class="cache-number-input"
              type="number"
              min="10"
              max="300"
              step="10"
            />
            <span class="cache-unit">秒</span>
            <button
              class="settings-action-btn"
              :disabled="isUpdatingSongCacheAheadSecs || isLoadingCacheStats"
              @click="applySongCacheAhead"
            >
              {{ isUpdatingSongCacheAheadSecs ? '应用中...' : '应用' }}
            </button>
          </div>
        </div>

        <div class="cache-breakdown-grid">
          <div v-for="item in cacheBreakdown" :key="item.label" class="cache-breakdown-item">
            <span class="cache-breakdown-label">{{ item.label }}</span>
            <strong>{{ item.usage }}</strong>
            <span>{{ item.count }}</span>
          </div>
        </div>

        <p v-if="cacheError" class="settings-status error">
          {{ cacheError }}
        </p>
      </div>
    </SettingGroup>

    <p v-if="message" class="settings-status" :class="messageType">
      {{ message }}
    </p>
  </template>

<style scoped>
.cache-card {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 20px;
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.4);
}

.cache-header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
}

.cache-label {
  margin: 0 0 6px;
  font-size: 12px;
  color: rgba(0, 0, 0, 0.45);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.cache-value {
  margin: 0;
  font-size: 24px;
  font-weight: 800;
  color: #111;
}

.cache-progress {
  height: 10px;
  background: rgba(0, 0, 0, 0.08);
  border-radius: 999px;
  overflow: hidden;
}

.cache-progress-fill {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, #111, #4b5563);
  transition: width 0.25s ease;
}

.cache-limit-row {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.cache-limit-label {
  font-size: 13px;
  font-weight: 700;
  color: rgba(0, 0, 0, 0.65);
}

.cache-limit-controls {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 96px auto auto;
  gap: 12px;
  align-items: center;
}

/* === DIV 自定义滑块样式 === */
.custom-slider-wrapper {
  position: relative;
  width: 100%;
  height: 20px; /* 点击热区高度，方便鼠标/手指操作 */
  display: flex;
  align-items: center;
}

.custom-slider-track {
  width: 100%;
  height: 6px; /* 轨道改细为 6px */
  background: rgba(0, 0, 0, 0.1); /* 灰色背景 */
  border-radius: 999px;
  overflow: hidden;
  pointer-events: none; /* 让鼠标事件穿透给底下的 input */
}

.custom-slider-fill {
  height: 100%;
  background: #111; /* 黑色进度条 */
  border-radius: inherit;
  /* 不加过渡效果，保证拖拽时极致跟手 */
}

.custom-slider-input {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  margin: 0;
  opacity: 0; /* 原生 input 完全透明化 */
  cursor: pointer;
}
/* ========================= */

.cache-number-input {
  width: 100%;
  padding: 8px 10px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.8);
  font-size: 13px;
  font-weight: 600;
}

.cache-unit {
  font-size: 12px;
  font-weight: 700;
  color: rgba(0, 0, 0, 0.5);
}

.cache-breakdown-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.cache-breakdown-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 14px 16px;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.55);
}

.cache-breakdown-item strong {
  font-size: 16px;
  color: #111;
}

.cache-breakdown-item span {
  font-size: 12px;
  color: rgba(0, 0, 0, 0.5);
}

.cache-breakdown-label {
  font-weight: 700;
  color: rgba(0, 0, 0, 0.75) !important;
}

@media (max-width: 900px) {
  .cache-limit-controls {
    grid-template-columns: 1fr;
  }

  .cache-breakdown-grid {
    grid-template-columns: 1fr;
  }
}
</style>
