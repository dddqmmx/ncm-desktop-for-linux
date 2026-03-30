<script setup lang="ts">
import AlbumCover from './AlbumCover.vue'

defineProps<{
  loading: boolean
  coverUrl?: string
  title?: string
  description?: string | null
  meta?: string[]
  searchPlaceholder?: string
}>()

const searchQuery = defineModel<string>('searchQuery', { default: '' })

defineEmits<{
  (e: 'playAll'): void
}>()
</script>

<template>
  <div class="main-content-scroll-wrapper">
    <!-- 加载状态 -->
    <div v-if="loading" class="loading-state">
      <div class="spinner"></div>
    </div>

    <div v-else class="playlist-container">
      <!-- 头部 -->
      <header class="playlist-header">
        <!-- 左侧：封面 -->
        <div class="cover-wrapper">
          <AlbumCover :key="coverUrl || 'placeholder'" :id="coverUrl" :alt="title" size="400y400" />
        </div>

        <!-- 右侧：详情与操作 -->
        <div class="playlist-details">
          <!-- 信息聚合区 (包裹起来是为了让底部的 action-bar 能被挤到最下面) -->
          <div class="info-group">
            <h1 class="playlist-title" :title="title">{{ title }}</h1>

            <!-- Creator Info -->
            <div v-if="$slots.creator" class="creator-row">
              <slot name="creator"></slot>
            </div>

            <!-- Meta Stats -->
            <div v-if="meta" class="stats-row">
              <template v-for="(item, index) in meta" :key="index">
                <span class="meta-text">{{ item }}</span>
                <span v-if="index < meta.length - 1" class="dot"></span>
              </template>
            </div>

            <p v-if="description" class="playlist-description">
              {{ description }}
            </p>
          </div>

          <!-- 操作栏：自动对齐到底部 -->
          <div class="action-bar">
            <button class="play-main-btn" @click="$emit('playAll')">
              <svg viewBox="0 0 24 24" fill="currentColor" width="20" height="20">
                <path d="M8 5v14l11-7z" />
              </svg>
              播放全部
            </button>

            <div class="search-box-container">
              <svg class="search-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <circle cx="11" cy="11" r="8" />
                <path d="m21 21-4.3-4.3" />
              </svg>
              <input v-model="searchQuery" type="text" :placeholder="searchPlaceholder || '在专辑内搜索...'" class="search-input" />
              <button v-if="searchQuery" class="clear-btn" @click="searchQuery = ''">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>

            <slot name="actions"></slot>

            <button class="icon-btn" title="更多">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="1.5" />
                <circle cx="19" cy="12" r="1.5" />
                <circle cx="5" cy="12" r="1.5" />
              </svg>
            </button>
          </div>
        </div>
      </header>

      <!-- 列表内容 -->
      <slot></slot>

      <div class="spacer-bottom"></div>
    </div>
  </div>
</template>

<style scoped>
.main-content-scroll-wrapper {
  height: 100%;
  overflow-y: auto;
  scrollbar-width: thin;
}

.main-content-scroll-wrapper::-webkit-scrollbar { width: 4px; }
.main-content-scroll-wrapper::-webkit-scrollbar-thumb { background: rgba(0, 0, 0, 0.1); border-radius: 10px; }

.playlist-container {
  padding: 40px; /* 增加一点内边距让呼吸感更好 */
  max-width: 1200px;
  margin: 0 auto;
  -webkit-app-region: no-drag;
}

/* 头部布局 */
.playlist-header {
  display: flex;
  gap: 40px; /* 缩减一点图文间距使其更紧凑 */
  align-items: stretch; /* 关键：让右侧详情区与左侧封面等高 */
  margin-bottom: 48px;
}

/* 封面强化 */
.cover-wrapper {
  width: 280px; /* 放大封面尺寸 (原240px) */
  height: 280px;
  flex-shrink: 0;
  border-radius: 12px; /* 稍微硬一点的圆角更显质感 */
  background: rgba(0, 0, 0, 0.03);
  /* 增强弥散阴影，营造悬浮感 */
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.15), 0 8px 16px rgba(0, 0, 0, 0.08);
  overflow: hidden;
  position: relative;
}

.cover-wrapper :deep(img) {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

/* 右侧详情区 */
.playlist-details {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0; /* 防止文本过长撑破 flex */
}

/* 文本信息打组 */
.info-group {
  display: flex;
  flex-direction: column;
  gap: 12px; /* 文本之间的行距收紧 */
  padding-top: 4px;
}

.playlist-title {
  font-size: 42px; /* 稍微调整字号以适应两行显示 */
  font-weight: 800;
  margin: 0;
  color: #111;
  letter-spacing: -1px;
  line-height: 1.2;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.stats-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  font-size: 14px;
  color: #555;
  font-weight: 500;
}
.meta-text { color: #666; }

.dot {
  width: 4px;
  height: 4px;
  background: #bbb;
  border-radius: 50%;
}

.playlist-description {
  font-size: 14px;
  color: #777;
  line-height: 1.6;
  margin: 8px 0 0 0;
  max-width: 90%;
  display: -webkit-box;
  -webkit-line-clamp: 2; /* 限制2行，超出的省略 */
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/* 操作栏：核心改进 */
.action-bar {
  margin-top: auto; /* 关键：自动顶到最底部，与封面底部水平对齐 */
  display: flex;
  align-items: center;
  gap: 16px; /* 按钮之间的间距 */
  -webkit-app-region: no-drag;
}

.play-main-btn {
  background: #111;
  color: #fff;
  border: none;
  height: 44px; /* 设定固定高度 */
  padding: 0 28px;
  border-radius: 100px; /* 完全圆角 */
  font-size: 15px;
  font-weight: 700;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  transition: transform 0.2s, background 0.2s;
}

.play-main-btn:hover {
  transform: scale(1.04);
  background: #000;
}

/* 搜索框风格与播放按钮统一 */
.search-box-container {
  position: relative;
  display: flex;
  align-items: center;
  background: rgba(0, 0, 0, 0.04);
  border-radius: 100px; /* 改为药丸形 */
  padding: 0 16px;
  height: 44px;
  transition: all 0.3s ease;
  width: 220px;
}

.search-box-container:focus-within {
  background: rgba(0, 0, 0, 0.06);
  width: 260px; /* 聚焦时微微变长 */
}

.search-icon { color: #888; margin-right: 8px; }

.search-input {
  background: transparent;
  border: none;
  outline: none;
  height: 100%;
  width: 100%;
  font-size: 13px;
  color: #333;
}

.search-input::placeholder { color: #999; }

.clear-btn {
  background: none;
  border: none;
  color: #999;
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
}

.clear-btn:hover { color: #333; }

/* 更多按钮去掉了边框，改为悬浮背景色 */
.icon-btn {
  background: transparent;
  border: none;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: #666;
  transition: background 0.2s;
}

.icon-btn:hover {
  background: rgba(0, 0, 0, 0.05);
  color: #111;
}

.loading-state { display: flex; justify-content: center; align-items: center; height: 300px; }
.spinner { width: 30px; height: 30px; border: 3px solid rgba(0,0,0,0.1); border-top-color: #111; border-radius: 50%; animation: spin 1s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
.spacer-bottom { height: 80px; }

@media (max-width: 900px) {
  .playlist-header { flex-direction: column; align-items: flex-start; }
  .cover-wrapper { width: 200px; height: 200px; }
  .playlist-details { min-height: auto; }
  .action-bar { margin-top: 24px; flex-wrap: wrap; width: 100%; }
  .search-box-container { width: 100%; }
}
</style>
