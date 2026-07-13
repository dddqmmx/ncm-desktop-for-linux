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
  isUpdatingSongMaxCacheAheadBytes,
  isClearingCache,
  cacheError,
  cacheLimitMb,
  songMaxCacheAheadMb
} = storeToRefs(configStore)

const message = ref('')
const messageType = ref<'success' | 'error'>('success')
const cacheLimitDraft = ref(cacheLimitMb.value)
const songMaxCacheAheadDraft = ref(songMaxCacheAheadMb.value)

watch(cacheLimitMb, (value) => {
  cacheLimitDraft.value = value
})

watch(songMaxCacheAheadMb, (value) => {
  songMaxCacheAheadDraft.value = value
})

// === 计算滑块进度的百分比用于 div 宽度渲染 ===
const cacheLimitPercent = computed(() => {
  const val = Number(cacheLimitDraft.value) || 128
  return Math.max(0, Math.min(100, ((val - 128) / (8192 - 128)) * 100))
})

const songMaxCacheAheadPercent = computed(() => {
  const val = Number(songMaxCacheAheadDraft.value) || 1
  return Math.max(0, Math.min(100, ((val - 1) / (128 - 1)) * 100))
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

const cacheBreakdown = computed(() => [
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

const applySongMaxCacheAhead = async (): Promise<void> => {
  const applied = await configStore.setSongMaxCacheAheadSize(songMaxCacheAheadDraft.value)
  messageType.value = applied ? 'success' : 'error'
  message.value = applied ? '歌曲最大预下载大小已更新。' : '歌曲最大预下载大小更新失败。'
}

const clearCache = async (): Promise<void> => {
  const cleared = await configStore.clearCache()
  messageType.value = cleared ? 'success' : 'error'
  message.value = cleared ? '缓存已清理。' : '缓存清理失败。'
}
</script>

<template>
  <div>
    <SettingGroup
      title="本地缓存"
      tip="native 缓存会维护歌曲、歌手/用户信息、封面和歌词并在超限时自动回收冷数据。"
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
          <label class="cache-limit-label" for="song-max-cache-ahead-range">最大预下载大小</label>
          <div class="cache-limit-controls">
            <div class="custom-slider-wrapper">
              <div class="custom-slider-track">
                <div
                  class="custom-slider-fill"
                  :style="{ width: songMaxCacheAheadPercent + '%' }"
                ></div>
              </div>
              <input
                id="song-max-cache-ahead-range"
                v-model.number="songMaxCacheAheadDraft"
                class="custom-slider-input"
                type="range"
                min="1"
                max="128"
                step="1"
              />
            </div>

            <input
              v-model.number="songMaxCacheAheadDraft"
              class="cache-number-input"
              type="number"
              min="1"
              max="128"
              step="1"
            />
            <span class="cache-unit">MB</span>
            <button
              class="settings-action-btn"
              :disabled="isUpdatingSongMaxCacheAheadBytes || isLoadingCacheStats"
              @click="applySongMaxCacheAhead"
            >
              {{ isUpdatingSongMaxCacheAheadBytes ? '应用中...' : '应用' }}
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
  </div>
</template>

<style scoped>
.cache-card {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 20px;
  border-radius: 18px;
  background: var(--sys-surface-muted);
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
  color: var(--sys-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.cache-value {
  margin: 0;
  font-size: 24px;
  font-weight: 800;
  color: var(--sys-text);
}

.cache-progress {
  height: 10px;
  background: var(--sys-control);
  border-radius: 999px;
  overflow: hidden;
}

.cache-progress-fill {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, var(--theme-color), var(--theme-color-strong));
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
  color: var(--sys-text-secondary);
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
  background: var(--sys-control);
  border-radius: 999px;
  overflow: hidden;
  pointer-events: none; /* 让鼠标事件穿透给底下的 input */
}

.custom-slider-fill {
  height: 100%;
  background: var(--theme-color);
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
  border: 1px solid var(--sys-border-strong);
  border-radius: 10px;
  background: var(--sys-surface-strong);
  color: var(--sys-text);
  font-size: 13px;
  font-weight: 600;
}

.cache-unit {
  font-size: 12px;
  font-weight: 700;
  color: var(--sys-text-tertiary);
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
  background: var(--sys-surface-muted);
}

.cache-breakdown-item strong {
  font-size: 16px;
  color: var(--sys-text);
  display: block;
}

.cache-breakdown-item span {
  font-size: 12px;
  color: var(--sys-text-tertiary);
  display: block;
}

.cache-breakdown-label {
  font-weight: 700;
  color: var(--sys-text-secondary) !important;
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
