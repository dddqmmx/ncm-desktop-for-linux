<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import { ref, nextTick, watch } from 'vue'
import { VueDraggable } from 'vue-draggable-plus'

const playerStore = usePlayerStore()
const listRef = ref<HTMLElement | null>(null)

/**
 * 自动滚动到当前播放项
 * 获取 TransitionGroup 内部的真实 DOM
 */
watch(() => playerStore.currentSong?.id, () => {
  nextTick(() => {
    // 因为 TransitionGroup 设置了 ref，它对应的是 .playlist-content 这个 div
    const activeItem = listRef.value?.querySelector('.playlist-item.active')
    if (activeItem) {
      activeItem.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  })
})

/**
 * 移除单曲
 */
const removeSong = (id: string | number): void => {
  const index = playerStore.playlist.findIndex(item => item.id === id)
  if (index !== -1) {
    playerStore.playlist.splice(index, 1)
  }
}

/**
 * 拖拽结束回调
 */
const onDragEnd = (): void => {
  console.log('新播放顺序已同步')
}
</script>

<template>
  <div class="playlist-card glass-morphism-heavy">
    <!-- 头部区域：固定高度 -->
    <div class="playlist-header">
      <div class="header-content">
        <span class="title">待播清单</span>
        <span class="count">{{ playerStore.playlist?.length || 0 }} 首歌曲</span>
        <button class="clear-btn" @click="playerStore.clearPlaylist">清空</button>
      </div>
    </div>

    <!--
      拖拽区域：设置为 flex: 1 填满剩余高度
      target=".playlist-content" 指向实际产生滚动条的容器
    -->
    <VueDraggable
      v-model="playerStore.playlist"
      target=".playlist-content"
      :animation="300"
      ghost-class="drag-ghost"
      drag-class="drag-active"
      class="draggable-container"
      @end="onDragEnd"
    >
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
          <div class="item-status">
            <div class="playing-icon" v-if="playerStore.currentSong?.id === song.id">
              <span class="bar" v-for="i in 3" :key="i"></span>
            </div>
            <div class="item-index" v-else>{{ index + 1 }}</div>
          </div>

          <img :src="song.cover" class="item-cover" draggable="false">

          <div class="item-info">
            <div class="item-name">{{ song.name }}</div>
            <div class="item-artist">{{ song.artist }}</div>
          </div>

          <div class="item-actions">
            <button class="action-btn remove" @click.stop="removeSong(song.id)">
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
/* --- 布局修复核心 --- */
.playlist-card {
  width: 360px;
  height: 520px;
  display: flex;
  flex-direction: column; /* 纵向排列：Header + Draggable */
  overflow: hidden;
  user-select: none;
}

.draggable-container {
  flex: 1; /* 关键：占据剩余全部高度 */
  min-height: 0; /* 关键：防止 flex 子元素被内容无限撑开 */
  display: flex;
  flex-direction: column;
}

.playlist-content {
  flex: 1;
  overflow-y: auto; /* 必须是 auto 或 scroll */
  padding: 0 12px 20px;
  /* 优化滚动体验 */
  scrollbar-gutter: stable;
}

/* --- 样式美化 --- */
/* --- 基础卡片样式 --- */
.glass-morphism-heavy {
  background: rgba(255, 255, 255, 0.4);
  backdrop-filter: blur(40px) saturate(180%);
  -webkit-backdrop-filter: blur(40px) saturate(180%);
  border: 0.5px solid rgba(255, 255, 255, 0.5);
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.08);
  border-radius: 28px;
}

.playlist-header {
  padding: 24px 24px 16px;
  flex-shrink: 0; /* 防止头部被压缩 */
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
  float: right;
}

.clear-btn:hover { background: rgba(255, 59, 48, 0.1); color: #ff3b30; }

/* --- 滚动条样式 --- */
.playlist-content::-webkit-scrollbar {
  width: 6px;
}
.playlist-content::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.1);
  border-radius: 10px;
}
.playlist-content::-webkit-scrollbar-track {
  background: transparent;
}

/* --- 列表项 --- */
.playlist-item {
  display: flex;
  align-items: center;
  padding: 10px 12px;
  border-radius: 18px;
  cursor: pointer;
  gap: 12px;
  margin-bottom: 4px;
  transition: all 0.2s;
    border: 1px solid transparent; /* 预留边框位置 */
}

.playlist-item:hover { background: rgba(0, 0, 0, 0.03); }
.playlist-item.active {
  /* 背景：更通透的白色，降低不透明度 */
  background: rgba(255, 255, 255, 0.5);

  /* 核心：背板模糊，数值 12px-20px 比较高级 */
  backdrop-filter: blur(12px) saturate(180%);
  -webkit-backdrop-filter: blur(12px) saturate(180%);

  /* 边框：模拟玻璃边缘受光的效果，上边框稍亮 */
  border: 1px solid rgba(255, 255, 255, 0.7);

  /* 阴影：一层深色投影增加悬浮感，一层白色内阴影增加厚度感 */
  box-shadow:
    0 8px 24px rgba(0, 0, 0, 0.08),
    inset 0 1px 1px rgba(255, 255, 255, 0.4);

  /* 微微放大，增加选中视觉差 */
  transform: scale(1.01);
  margin-top: 2px;
  margin-bottom: 6px;
  z-index: 2;
}

/* 选中项的文字颜色微调 */
.playlist-item.active .item-name {
  color: #000; /* 让标题更深 */
}

.playlist-item.active .item-artist {
  color: rgba(0, 0, 0, 0.6);
}

.item-status { width: 24px; display: flex; justify-content: center; }
.item-index { font-size: 11px; font-weight: 700; color: rgba(0,0,0,0.2); }
.item-cover { width: 40px; height: 40px; border-radius: 10px; object-fit: cover; }
.item-info { flex: 1; min-width: 0; }
.item-name { font-size: 13px; font-weight: 600; color: #111; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.item-artist { font-size: 11px; color: rgba(0,0,0,0.5); }

.item-actions { opacity: 0; }
.playlist-item:hover .item-actions { opacity: 1; }
.action-btn { background: none; border: none; cursor: pointer; color: #999; }
.action-btn:hover { color: #ff3b30; }

/* --- 动画 --- */
.list-anim-enter-active, .list-anim-leave-active { transition: all 0.3s ease; }
.list-anim-enter-from, .list-anim-leave-to { opacity: 0; transform: translateX(20px); }
/* 确保删除时其他元素平滑移动 */
.list-anim-leave-active { position: absolute; width: 312px; }
.list-anim-move { transition: transform 0.3s ease; }

.drag-ghost { opacity: 0.3; background: #eee !important; }
.drag-active { transform: scale(1.02); z-index: 999; }

/* --- 播放状态柱状图 --- */
.playing-icon { display: flex; align-items: flex-end; gap: 2px; height: 12px; }
.playing-icon .bar { width: 3px; background: #111; border-radius: 1px; animation: bounce 0.6s infinite alternate; }
.playing-icon .bar:nth-child(2) { animation-delay: 0.2s; }
.playing-icon .bar:nth-child(3) { animation-delay: 0.4s; }
@keyframes bounce { from { height: 4px; } to { height: 12px; } }

.empty-state {
  position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%);
  text-align: center; color: #ccc;
}
</style>
