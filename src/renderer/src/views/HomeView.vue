<script setup lang="ts">
import MediaCard from '../components/MediaCard.vue'
import AlbumItem from '../components/AlbumItem.vue'
import { onMounted, ref } from 'vue'
import { useUserStore } from '@renderer/stores/userStore'
import { RecommendResource, RecommendItem } from '@renderer/types/recommendResource'
import router from '@renderer/router'
import { RecommendSongs, Song } from '@renderer/types/recommendSongs'
import { usePlayerStore } from '@renderer/stores/playerStore'
import { resolveCachedMediaUrl } from '@renderer/utils/cache'

const userStore = useUserStore()
const playerStore = usePlayerStore()
const recommendPlaylists = ref<RecommendItem[]>([])
const dailySongs = ref<Song[]>([])

onMounted(async (): Promise<void> => {
  try {
    const recommendResource = (await window.api.recommend_resource({
      cookie: userStore.cookie
    })) as { body?: RecommendResource }
    if (recommendResource.body?.recommend) {
      recommendPlaylists.value = await Promise.all(
        recommendResource.body.recommend.map(
          async (item): Promise<RecommendItem> => ({
            ...item,
            picUrl: await resolveCachedMediaUrl(item.picUrl),
            creator: {
              ...item.creator,
              avatarUrl: await resolveCachedMediaUrl(item.creator.avatarUrl)
            }
          })
        )
      )
    }

    const recommendSongsRes = (await window.api.recommend_songs({
      cookie: userStore.cookie
    })) as { body?: RecommendSongs }
    if (recommendSongsRes.body?.data?.dailySongs) {
      dailySongs.value = await Promise.all(
        recommendSongsRes.body.data.dailySongs.map(
          async (song): Promise<Song> => ({
            ...song,
            al: {
              ...song.al,
              picUrl: await resolveCachedMediaUrl(song.al.picUrl + '?param=200y200')
            }
          })
        )
      )
    }
  } catch (error) {
    console.error('获取推荐内容失败:', error)
  }
})

/**
 * 处理滚轮事件：将垂直滚动转换为水平滚动
 * 增加了加速度处理，使滚动更加流畅
 */
const handleWheel = (e: WheelEvent): void => {
  const container = e.currentTarget as HTMLElement
  if (Math.abs(e.deltaX) < Math.abs(e.deltaY)) {
    // 使用 deltaY 进行滚动，并增加一定的倍率以提升响应感
    // 浏览器原生的 deltaY 已经包含了速度信息
    container.scrollLeft += e.deltaY * 1.5
    e.preventDefault()
  }
}

const formatArtists = (artists: Song['ar']): string =>
  artists.map((artist) => artist.name).join(' / ')
const playSong = (song: Song): void => {
  void playerStore.playMusic(song.id)
}
</script>

<template>
  <main class="scrollable-content">
    <h1 class="page-title">主页</h1>

    <!-- 1. 每日推荐歌单部分 -->
    <section v-if="userStore.isLoggedIn" class="section">
      <div class="section-header">
        <h2>每日推荐歌单</h2>
      </div>

      <div ref="playlistRow" class="horizontal-scroll-container" @wheel="handleWheel">
        <div class="cards-grid">
          <MediaCard
            v-for="(item, index) in recommendPlaylists"
            :key="item.id"
            :title="item.name"
            :desc="item.copywriter || item.creator.nickname"
            :image="item.picUrl"
            :is-first="index === 0"
            type="playlist"
            @click="router.push('/playlist/' + item.id)"
          />
        </div>
      </div>
    </section>

    <!-- 2. 每日推荐歌曲部分 -->
    <section class="section">
      <div class="section-header">
        <h2>每日推荐歌曲</h2>
      </div>

      <div ref="songsRow" class="horizontal-scroll-container" @wheel="handleWheel">
        <div v-if="dailySongs.length === 0" class="loading-text">加载中...</div>
        <div class="albums-row">
          <AlbumItem
            v-for="song in dailySongs"
            :key="song.id"
            :title="song.name"
            :artist="formatArtists(song.ar)"
            :cover="song.al.picUrl"
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
  -webkit-app-region: no-drag;
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
  /* 移除强制平滑，让手动 Wheel 事件能快速响应并配合 Snap */
  scroll-behavior: auto;
  scroll-snap-type: x mandatory;
  scroll-padding-left: 24px;
  scrollbar-width: none;
  -webkit-app-region: no-drag;
  margin: -12px -24px;
  padding: 12px 24px;
}

.horizontal-scroll-container::-webkit-scrollbar {
  display: none;
  /* Chrome/Safari */
}

/* 内部内容布局 */
.cards-grid,
.albums-row {
  display: flex;
  gap: 20px;
  align-items: flex-start;
}

.cards-grid > * {
  flex: 0 0 auto;
  scroll-snap-align: start;
  width: 200px; /* 推荐歌单卡片宽度 */
}

.albums-row > * {
  flex: 0 0 auto;
  scroll-snap-align: start;
  width: 176px; /* 推荐歌曲卡片宽度，与 AlbumItem 内部一致 */
}

.loading-text {
  color: #888;
  padding: 40px 0;
}

.spacer-bottom {
  height: 100px;
}
</style>
