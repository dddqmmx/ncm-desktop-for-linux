<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import { ref, nextTick, watch } from 'vue'
import { VueDraggable } from 'vue-draggable-plus'

const playerStore = usePlayerStore()
const listRef = ref<HTMLElement | null>(null)

/**
 * 自动滚动到当前播放项
 */
watch(() => playerStore.currentSong?.id, () => {
  nextTick(() => {
    const activeItem = listRef.value?.querySelector('.playlist-item.active')
    activeItem?.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
  })
})

/**
 * 移除单曲
 * @param id 歌曲ID
 */
const removeSong = (id: string | number) => {
  const index = playerStore.playlist.findIndex(item => item.id === id)
  if (index !== -1) {
    playerStore.playlist.splice(index, 1)
  }
}

/**
 * 拖拽结束回调
 */
const onDragEnd = () => {
  console.log('新播放顺序已同步')
  // 这里可以调用 playerStore.savePlaylistToLocal() 持久化顺序
}
</script>

<template>
  <div class="playlist-card glass-morphism-heavy">
    <!-- 头部区域 -->
    <div class="playlist-header">
      <div class="header-content">
        <span class="title">待播清单</span>
        <span class="count">{{ playerStore.playlist?.length || 0 }} 首歌曲</span>
      </div>
      <button class="clear-btn" @click="playerStore.clearPlaylist">清空</button>
    </div>

    <!-- 拖拽组件：target 指向内部的滚动容器 -->
    <VueDraggable
      v-model="playerStore.playlist"
      target=".playlist-content"
      :animation="300"
      ghost-class="drag-ghost"
      drag-class="drag-active"
      @end="onDragEnd"
    >
      <!-- TransitionGroup 提供列表移动和移除动画 -->
      <TransitionGroup
        type="transition"
        name="list-anim"
        tag="div"
        class="playlist-content"
        ref="listRef"
      >
        <div
          v-for="(song, index) in playerStore.playlist"
          :key="song.id"
          class="playlist-item"
          :class="{ active: playerStore.currentSong?.id === song.id }"
          @click="playerStore.playMusic(song.id)"
        >
          <!-- 状态标识/序号 -->
          <div class="item-status">
            <div class="playing-icon" v-if="playerStore.currentSong?.id === song.id">
              <span class="bar" v-for="i in 3" :key="i"></span>
            </div>
            <div class="item-index" v-else>{{ index + 1 }}</div>
          </div>

          <!-- 封面图 (禁止原生拖拽干扰) -->
          <img :src="song.cover" class="item-cover" draggable="false">

          <!-- 歌曲信息 -->
          <div class="item-info">
            <div class="item-name">{{ song.name }}</div>
            <div class="item-artist">{{ song.artist }}</div>
          </div>

          <!-- 操作按钮区 -->
          <div class="item-actions">
            <button class="action-btn remove" @click.stop="removeSong(song.id)" title="移除">
              <svg viewBox="0 0 24 24" width="16" height="16">
                <path fill="currentColor" d="M19,6.41L17.59,5L12,10.59L6.41,5L5,6.41L10.59,12L5,17.59L6.41,19L12,13.41L17.59,19L19,17.59L13.41,12L19,6.41Z" />
              </svg>
            </button>
          </div>
        </div>
      </TransitionGroup>
    </VueDraggable>

    <!-- 空状态 -->
    <div v-if="!playerStore.playlist?.length" class="empty-state">
      <div class="empty-icon">♪</div>
      暂无待播歌曲
    </div>
  </div>
</template>

<style scoped>
/* --- 基础卡片样式 --- */
.glass-morphism-heavy {
  background: rgba(255, 255, 255, 0.4);
  backdrop-filter: blur(40px) saturate(180%);
  -webkit-backdrop-filter: blur(40px) saturate(180%);
  border: 0.5px solid rgba(255, 255, 255, 0.5);
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.08);
  border-radius: 28px;
}

.playlist-card {
  width: 360px;
  height: 520px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  user-select: none;
}

.playlist-header {
  padding: 24px 24px 16px;
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
}

.title { font-size: 18px; font-weight: 800; color: #111; }
.count { font-size: 12px; color: rgba(0,0,0,0.4); font-weight: 600; margin-left: 8px; }

.clear-btn {
  background: rgba(0,0,0,0.05);
  border: none;
  padding: 6px 14px;
  border-radius: 10px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}
.clear-btn:hover { background: rgba(255, 59, 48, 0.1); color: #ff3b30; }

/* --- 列表容器 --- */
.playlist-content {
  flex: 1;
  overflow-y: overlay; /* 兼容部分浏览器滚动条不占位 */
  padding: 0 12px 20px;
}

.playlist-content::-webkit-scrollbar { width: 4px; }
.playlist-content::-webkit-scrollbar-thumb {
  background: rgba(0,0,0,0.1);
  border-radius: 10px;
}

/* --- 列表项样式 --- */
.playlist-item {
  display: flex;
  align-items: center;
  padding: 10px 12px;
  border-radius: 18px;
  cursor: pointer;
  transition: background 0.2s, transform 0.1s;
  gap: 12px;
  margin-bottom: 2px;
  position: relative;
  /* 防止拖拽时文字被选中 */
  user-select: none;
}

.playlist-item:hover {
  background: rgba(255, 255, 255, 0.4);
}

.playlist-item.active {
  background: white;
  box-shadow: 0 4px 12px rgba(0,0,0,0.05);
}

/* 状态与索引 */
.item-status { width: 24px; display: flex; justify-content: center; }
.item-index { font-size: 11px; font-weight: 700; color: rgba(0,0,0,0.2); }

/* 封面与信息 */
.item-cover { width: 40px; height: 40px; border-radius: 10px; object-fit: cover; pointer-events: none; }
.item-info { flex: 1; min-width: 0; }
.item-name { font-size: 13px; font-weight: 600; color: #111; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.item-artist { font-size: 11px; color: rgba(0,0,0,0.5); margin-top: 2px; }

/* 移除按钮 */
.item-actions { opacity: 0; transition: opacity 0.2s; }
.playlist-item:hover .item-actions { opacity: 1; }

.action-btn {
  background: none; border: none; padding: 6px; border-radius: 8px;
  color: rgba(0,0,0,0.3); cursor: pointer; display: flex;
}
.action-btn.remove:hover { background: rgba(255, 59, 48, 0.1); color: #ff3b30; }

/* --- 拖拽专属样式 --- */
/* 正在被拖拽的元素 */
.drag-active {
  opacity: 1 !important;
  background: white !important;
  box-shadow: 0 12px 30px rgba(0,0,0,0.15) !important;
  transform: scale(1.04);
  z-index: 100;
}
/* 占位符样式 */
.drag-ghost {
  opacity: 0.2;
  background: rgba(0,0,0,0.1) !important;
}

/* --- 动画部分 --- */

/* 列表移动动画 (当其他元素位移时) */
.list-anim-move {
  transition: transform 0.4s cubic-bezier(0.3, 0, 0, 1);
}

/* 移除/进入动画 */
.list-anim-enter-active,
.list-anim-leave-active {
  transition: all 0.4s cubic-bezier(0.3, 0, 0, 1);
}

.list-anim-enter-from {
  opacity: 0;
  transform: translateX(30px);
}

.list-anim-leave-to {
  opacity: 0;
  transform: translateX(-100%); /* 优雅地向左滑出 */
}

/* 正在移除的元素脱离文档流，确保其他元素平滑上移 */
.list-anim-leave-active {
  position: absolute;
  width: calc(100% - 24px); /* 减去 padding */
  z-index: 0;
}

/* --- 播放状态柱状图 --- */
.playing-icon { display: flex; align-items: flex-end; gap: 2px; height: 12px; }
.playing-icon .bar { width: 3px; background: #111; border-radius: 1px; animation: bounce 0.6s infinite alternate; }
.playing-icon .bar:nth-child(2) { animation-delay: 0.2s; }
.playing-icon .bar:nth-child(3) { animation-delay: 0.4s; }

@keyframes bounce { from { height: 4px; } to { height: 12px; } }

.empty-state {
  flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
  font-size: 13px; color: rgba(0,0,0,0.3); gap: 12px;
}
.empty-icon { font-size: 40px; opacity: 0.2; }
</style>
