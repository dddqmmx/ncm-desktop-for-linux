<script setup lang="ts">
import MediaCard from '../components/MediaCard.vue'
import AlbumItem from '../components/AlbumItem.vue'
import { onMounted, ref } from 'vue';
import { useUserStore } from '@renderer/stores/userStore';
import { RecommendResource, RecommendItem } from '@renderer/types/recommendResource';
import router from '@renderer/router';
import { RecommendSongs, Song } from '@renderer/types/recommendSongs';
import { usePlayerStore } from '@renderer/stores/playerStore';

const userStore = useUserStore()
const playerStore = usePlayerStore()
const recommendPlaylists = ref<RecommendItem[]>([])
const dailySongs = ref<Song[]>([])

// 引用 DOM 元素用于控制滚动
const playlistRow = ref<HTMLElement | null>(null)
const songsRow = ref<HTMLElement | null>(null)

onMounted(async () => {
  try {
    const recommendResource = await window.api.recommend_resource({
      cookie: userStore.cookie
    }) as { body?: RecommendResource }
    if (recommendResource.body?.recommend) {
      recommendPlaylists.value = recommendResource.body.recommend
    }

    const recommendSongsRes = await window.api.recommend_songs({
      cookie: userStore.cookie
    }) as { body?: RecommendSongs }
    if (recommendSongsRes.body?.data?.dailySongs) {
      dailySongs.value = recommendSongsRes.body.data.dailySongs
    }
  } catch (error) {
    console.error('获取推荐内容失败:', error)
  }
})

/**
 * 处理滚轮事件：将垂直滚动转换为水平滚动
 */
const handleWheel = (e: WheelEvent) => {
  const container = e.currentTarget as HTMLElement
  if (Math.abs(e.deltaX) < Math.abs(e.deltaY)) {
    container.scrollLeft += e.deltaY
    e.preventDefault()
  }
}

/**
 * 按钮点击滚动
 */
const scrollByButton = (refElement: HTMLElement | null, direction: 'left' | 'right') => {
  if (!refElement) return
  const scrollAmount = refElement.clientWidth * 0.8 // 每次滚动容器宽度的80%
  refElement.scrollBy({
    left: direction === 'left' ? -scrollAmount : scrollAmount,
    behavior: 'smooth'
  })
}

const formatArtists = (artists: any[]) => artists.map(ar => ar.name).join(' / ')
const playSong = (song: Song) => playerStore.playMusic(song.id)
</script>

<template>
  <main class="scrollable-content">
    <h1 class="page-title">主页</h1>

    <!-- 1. 每日推荐歌单部分 -->
    <section class="section">
      <div class="section-header">
        <h2>每日推荐歌单</h2>
        <div class="nav-btns">
          <button @click="scrollByButton(playlistRow, 'left')" class="nav-btn">‹</button>
          <button @click="scrollByButton(playlistRow, 'right')" class="nav-btn">›</button>
        </div>
      </div>

      <div
        class="horizontal-scroll-container"
        ref="playlistRow"
        @wheel="handleWheel"
      >
        <div class="cards-grid">
          <MediaCard
            v-for="(item, index) in recommendPlaylists"
            :key="item.id"
            :title="item.name"
            :desc="item.copywriter || item.creator.nickname"
            :image="item.picUrl"
            :is-first="index === 0"
            type="playlist"
            @click="router.push('/playlist/'+item.id)"
          />
        </div>
      </div>
    </section>

    <!-- 2. 每日推荐歌曲部分 -->
    <section class="section">
      <div class="section-header">
        <h2>每日推荐歌曲</h2>
        <div class="nav-btns">
          <button @click="scrollByButton(songsRow, 'left')" class="nav-btn">‹</button>
          <button @click="scrollByButton(songsRow, 'right')" class="nav-btn">›</button>
        </div>
      </div>

      <div
        class="horizontal-scroll-container"
        ref="songsRow"
        @wheel="handleWheel"
      >
        <div v-if="dailySongs.length === 0" class="loading-text">加载中...</div>
        <div class="albums-row">
          <AlbumItem
            v-for="song in dailySongs"
            :key="song.id"
            :title="song.name"
            :artist="formatArtists(song.ar)"
            :cover="song.al.picUrl + '?param=200y200'"
            @click="playSong(song)"
          />
        </div>
      </div>
    </section>

    <div class="spacer-bottom"></div>
  </main>
</template>

<style scoped>
/* 容器基础样式 */
.scrollable-content {
  padding: 20px 24px 0 24px;
  overflow-y: auto;
  height: 100%;
  box-sizing: border-box;
}

.page-title {
  font-size: 32px;
  font-weight: 800;
  margin-bottom: 24px;
  letter-spacing: -0.5px;
}

.section {
  margin-bottom: 40px;
  position: relative;
}

/* 头部样式 */
.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.section-header h2 {
  font-size: 22px;
  font-weight: 700;
}

.nav-btns {
  display: flex;
  gap: 8px;
}

.nav-btn {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  border: none;
  background: rgba(255, 255, 255, 0.1);
  color: white;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  transition: all 0.2s;
}

.nav-btn:hover {
  background: rgba(255, 255, 255, 0.2);
  transform: scale(1.05);
}

/* 核心滚动容器 - 修复截断的关键 */
.horizontal-scroll-container {
  overflow-x: auto;
  scroll-behavior: smooth;
  scroll-snap-type: x mandatory;
  scrollbar-width: none; /* Firefox */

  /*
     关键修复：
     1. 通过 padding 撑开容器内部空间，允许子元素向上浮动 12px 而不被截断
     2. 通过 margin 抵消 padding 占据的外部空间，保持与标题的间距不变
  */
  margin: -12px -24px;
  padding: 12px 24px;
}

.horizontal-scroll-container::-webkit-scrollbar {
  display: none; /* Chrome/Safari */
}

/* 内部内容布局 */
.cards-grid, .albums-row {
  display: flex;
  gap: 20px;
  /* 确保子元素顶部对齐，防止被拉伸 */
  align-items: flex-start;
}

/* 保持原有宽度逻辑不变 */
.cards-grid > *, .albums-row > * {
  flex: 0 0 auto;
  scroll-snap-align: start;
  width: 180px;
}

/* 针对 MediaCard 的特殊宽度处理 */
.cards-grid :deep(.large-card) {
  /* 这里建议直接指向子组件内的类名，或者保持你之前的 width: 200px */
  width: 200px;
}

.loading-text {
  color: #888;
  padding: 40px 0;
}

.spacer-bottom {
  height: 100px;
}
</style>
