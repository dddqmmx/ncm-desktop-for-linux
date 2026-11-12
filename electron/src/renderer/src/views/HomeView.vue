<script setup lang="ts">
import MediaCard from '@renderer/components/media/MediaCard.vue'
import AlbumItem from '@renderer/components/media/AlbumItem.vue'
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
// 两个板块各自加载：谁先回来谁先替换掉自己的骨架，不必互相等。
const playlistsLoading = ref(true)
const songsLoading = ref(true)

// 骨架数量按常见窗口宽度铺满一行以上，加载完成后卡片替换骨架时高度不变，
// 下面的板块不会被顶下去再弹回来。
const PLAYLIST_SKELETON_COUNT = 6
const SONG_SKELETON_COUNT = 8

const loadRecommendPlaylists = async (): Promise<void> => {
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
  } catch (error) {
    console.error('获取推荐歌单失败:', error)
  } finally {
    playlistsLoading.value = false
  }
}

const loadDailySongs = async (): Promise<void> => {
  try {
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
    console.error('获取每日推荐歌曲失败:', error)
  } finally {
    songsLoading.value = false
  }
}

onMounted(async (): Promise<void> => {
  await Promise.all([loadRecommendPlaylists(), loadDailySongs()])
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

      <div
        ref="playlistRow"
        class="horizontal-scroll-container"
        :aria-busy="playlistsLoading"
        @wheel="handleWheel"
      >
        <div v-if="playlistsLoading" class="cards-grid" aria-hidden="true">
          <div v-for="n in PLAYLIST_SKELETON_COUNT" :key="n" class="card-skeleton"></div>
        </div>
        <div v-else class="cards-grid">
          <MediaCard
            v-for="item in recommendPlaylists"
            :key="item.id"
            :title="item.name"
            :desc="item.copywriter || item.creator.nickname"
            :image="item.picUrl"
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

      <div
        ref="songsRow"
        class="horizontal-scroll-container"
        :aria-busy="songsLoading"
        @wheel="handleWheel"
      >
        <div v-if="songsLoading" class="albums-row" aria-hidden="true">
          <div v-for="n in SONG_SKELETON_COUNT" :key="n" class="album-skeleton">
            <div class="album-skeleton-cover"></div>
            <div class="album-skeleton-title"></div>
            <div class="album-skeleton-artist"></div>
          </div>
        </div>
        <div v-else class="albums-row">
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
  letter-spacing: 0;
  color: var(--sys-text);
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
  color: var(--sys-text);
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
  background: var(--sys-control);
  color: var(--sys-text);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  transition: all 0.2s;
}

.nav-btn:hover {
  background: var(--sys-control-hover);
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

/* 骨架：尺寸与真实卡片一致（歌单卡 200×250，歌曲卡 176 宽 + 封面 + 两行文字），
   替换时不改变行高，下方的板块不会被顶下去。只用既有中性色，不引入新色相。 */
.card-skeleton,
.album-skeleton-cover,
.album-skeleton-title,
.album-skeleton-artist {
  background: var(--sys-control);
  position: relative;
  overflow: hidden;
}

/* 一道低对比度的扫光，表示"正在加载"而不是"加载失败" */
.card-skeleton::after,
.album-skeleton-cover::after,
.album-skeleton-title::after,
.album-skeleton-artist::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(
    90deg,
    transparent 0%,
    var(--sys-surface-muted) 50%,
    transparent 100%
  );
  transform: translateX(-100%);
  animation: skeleton-sweep 1.4s ease-in-out infinite;
}

.card-skeleton {
  aspect-ratio: 1 / 1.25;
  border-radius: 18px;
}

.album-skeleton-cover {
  width: 100%;
  aspect-ratio: 1;
  border-radius: 28px;
  margin-bottom: 14px;
}

.album-skeleton-title {
  height: 18px;
  border-radius: 6px;
  margin: 0 6px;
}

.album-skeleton-artist {
  height: 14px;
  width: 60%;
  border-radius: 6px;
  margin: 12px 6px 0;
}

@keyframes skeleton-sweep {
  to {
    transform: translateX(100%);
  }
}

@media (prefers-reduced-motion: reduce) {
  .card-skeleton::after,
  .album-skeleton-cover::after,
  .album-skeleton-title::after,
  .album-skeleton-artist::after {
    animation: none;
  }
}

.spacer-bottom {
  height: 100px;
}
</style>
