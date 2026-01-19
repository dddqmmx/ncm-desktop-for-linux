<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import { computed, watch, ref, nextTick } from 'vue'

const playerStore = usePlayerStore()

// 自动滚动到当前播放的歌曲
const listRef = ref<HTMLElement | null>(null)
watch(() => playerStore.currentSong?.id, () => {
  nextTick(() => {
    const activeItem = listRef.value?.querySelector('.playlist-item.active')
    activeItem?.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
  })
})
</script>

<template>
  <div class="playlist-card glass-morphism-heavy">
    <div class="playlist-header">
      <div class="header-content">
        <span class="title">待播清单</span>
        <span class="count">{{ playerStore.playlist?.length || 0 }} 首歌曲</span>
      </div>
      <button class="clear-btn" @click="playerStore.clearPlaylist">清空</button>
    </div>

    <div class="playlist-content" ref="listRef">
      <div
        v-for="(song, index) in playerStore.playlist"
        :key="song.id || index"
        class="playlist-item"
        :class="{ active: playerStore.currentSong?.id === song.id }"
        @click="playerStore.playMusic(song.id )"
      >
        <div class="item-index" v-if="playerStore.currentSong?.id !== song.id">
          {{ index + 1 }}
        </div>
        <div class="playing-icon" v-else>
          <span class="bar" v-for="i in 3" :key="i"></span>
        </div>

        <img :src="song.cover" class="item-cover">

        <div class="item-info">
          <div class="item-name">{{ song.name }}</div>
          <div class="item-artist">{{ song.artist }}</div>
        </div>

        <div class="item-duration" >
          <!-- v-if="song.dt"     {{ Math.floor(song.dt / 1000 / 60) }}:{{ String(Math.floor((song.dt / 1000) % 60)).padStart(2, '0') }} -->
        </div>
      </div>

      <div v-if="!playerStore.playlist?.length" class="empty-state">
        暂无待播歌曲
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 强化版的玻璃质感 */
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
  max-height: 480px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  margin-bottom: 12px; /* 与播放器栏的间距 */
  transform-origin: bottom right;
}

/* 头部 */
.playlist-header {
  padding: 20px 24px 12px;
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
}

.title {
  display: block;
  font-size: 18px;
  font-weight: 800;
  color: #111;
}

.count {
  font-size: 12px;
  color: rgba(0,0,0,0.4);
  font-weight: 600;
}

.clear-btn {
  background: rgba(0,0,0,0.05);
  border: none;
  padding: 6px 12px;
  border-radius: 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.clear-btn:hover {
  background: rgba(255, 59, 48, 0.1);
  color: #ff3b30;
}

/* 列表区域 */
.playlist-content {
  flex: 1;
  overflow-y: auto;
  padding: 0 12px 20px;
}

/* 隐藏滚动条但保留功能 */
.playlist-content::-webkit-scrollbar {
  width: 4px;
}
.playlist-content::-webkit-scrollbar-thumb {
  background: rgba(0,0,0,0.1);
  border-radius: 10px;
}

.playlist-item {
  display: flex;
  align-items: center;
  padding: 10px 12px;
  border-radius: 16px;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  gap: 12px;
}

.playlist-item:hover {
  background: rgba(255, 255, 255, 0.4);
}

.playlist-item.active {
  background: white;
  box-shadow: 0 4px 12px rgba(0,0,0,0.05);
}

.item-index {
  width: 20px;
  font-size: 11px;
  font-weight: 700;
  color: rgba(0,0,0,0.2);
  text-align: center;
}

.item-cover {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  object-fit: cover;
}

.item-info {
  flex: 1;
  overflow: hidden;
}

.item-name {
  font-size: 13px;
  font-weight: 600;
  color: #111;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.item-artist {
  font-size: 11px;
  color: rgba(0,0,0,0.5);
}

.item-duration {
  font-size: 11px;
  color: rgba(0,0,0,0.3);
  font-family: 'Monaco', monospace;
}

/* 正在播放动画 */
.playing-icon {
  width: 20px;
  display: flex;
  align-items: flex-end;
  justify-content: center;
  gap: 2px;
  height: 12px;
}
.playing-icon .bar {
  width: 3px;
  background: #111;
  border-radius: 1px;
  animation: bounce 0.6s infinite alternate;
}
.playing-icon .bar:nth-child(2) { animation-delay: 0.2s; }
.playing-icon .bar:nth-child(3) { animation-delay: 0.4s; }

@keyframes bounce {
  from { height: 4px; }
  to { height: 12px; }
}

.empty-state {
  text-align: center;
  padding: 40px 0;
  font-size: 13px;
  color: rgba(0,0,0,0.3);
}
</style>
