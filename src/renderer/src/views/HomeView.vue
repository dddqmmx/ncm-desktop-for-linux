<script setup lang="ts">
import MediaCard from '../components/MediaCard.vue'
import AlbumItem from '../components/AlbumItem.vue'
import { onMounted, ref } from 'vue';
import { useUserStore } from '@renderer/stores/userStore';
import { RecommendResource, RecommendItem } from '@renderer/types/recommendResource';
import router from '@renderer/router';
// 导入定义的接口
import { RecommendSongs, Song } from '@renderer/types/recommendSongs';
import { usePlayerStore } from '@renderer/stores/playerStore';

const userStore = useUserStore()
const playerStore = usePlayerStore()
const recommendPlaylists = ref<RecommendItem[]>([])
// 1. 定义推荐歌曲的响应式变量
const dailySongs = ref<Song[]>([])

onMounted(async () => {
  try {
    // 获取推荐歌单
    const recommendResource = await window.api.recommend_resource({
      cookie: userStore.cookie
    }) as { body?: RecommendResource }

    if (recommendResource.body && recommendResource.body.recommend) {
      recommendPlaylists.value = recommendResource.body.recommend
    }

    // 2. 获取每日推荐歌曲
    const recommendSongsRes = await window.api.recommend_songs({
      cookie: userStore.cookie
    }) as { body?: RecommendSongs }

    // 3. 赋值给 dailySongs
    if (recommendSongsRes.body?.data?.dailySongs) {
      dailySongs.value = recommendSongsRes.body.data.dailySongs
    }
  } catch (error) {
    console.error('获取推荐内容失败:', error)
  }
})

/**
 * 格式化歌手名字：将多个歌手用 / 隔开
 */
const formatArtists = (artists: any[]) => {
  return artists.map(ar => ar.name).join(' / ')
}

/**
 * 播放歌曲的处理函数
 */
const playSong = (song: Song) => {
  playerStore.playMusic(song.id)
}
</script>

<template>
  <main class="scrollable-content">
    <h1 class="page-title">主页</h1>

    <!-- 每日推荐歌单部分保持不变... -->
    <section class="section">
      <div class="section-header">
        <h2>每日推荐歌单</h2>
      </div>
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
    </section>

    <!-- 3. 修改每日推荐歌曲展示 -->
    <section class="section">
      <div class="section-header-simple">
        <h2>每日推荐歌曲 <span class="chevron">›</span></h2>
      </div>
      <div class="albums-row">
        <!-- 如果数据还没加载出来，显示加载状态 -->
        <div v-if="dailySongs.length === 0" class="loading-text">加载中...</div>

        <AlbumItem
          v-for="song in dailySongs"
          :key="song.id"
          :title="song.name"
          :artist="formatArtists(song.ar)"
          :cover="song.al.picUrl + '?param=200y200'"
          @click="playSong(song)"
        />
      </div>
    </section>

    <div class="spacer-bottom"></div>
  </main>
</template>

<style scoped>
.scrollable-content {
  padding: 20px 16px 0 10px;
  overflow-y: auto;
  flex: 1;
}

.page-title {
  font-size: 34px;
  font-weight: 800;
  margin-bottom: 24px;
}

.section { margin-bottom: 40px; }

.section-header h2,
.section-header-simple h2 {
  font-size: 22px;
  font-weight: 700;
  margin-bottom: 10px;
}

/* 修改后的关键 CSS */
.cards-grid {
  display: flex;
  overflow-x: auto;
  gap: 24px;
  padding-bottom: 15px; /* 增加底部填充防止截断阴影 */
  scroll-behavior: smooth;
  /* 隐藏滚动条 */
  scrollbar-width: none;
}

.cards-grid::-webkit-scrollbar {
  display: none;
}

/* 确保子卡片保持固定宽度 */
.cards-grid :deep(> *) {
  flex: 0 0 240px;
}

.albums-row {
  display: flex;
  gap: 24px;
  overflow-x: auto;
  padding-bottom: 20px;
}

.albums-row {
  display: flex;
  gap: 24px;
  overflow-x: auto;
  padding-bottom: 20px;
  /* 默认隐藏轨道和滑块 */
}

/* 默认状态：滑块透明 */
.albums-row::-webkit-scrollbar-thumb {
  background-color: transparent;
  border-radius: 10px;
}

/* 鼠标移入容器时：显示滑块 */
.albums-row:hover::-webkit-scrollbar-thumb {
  background-color: rgba(155, 155, 155, 0.4);
}

.albums-row::-webkit-scrollbar {
  height: 6px;
}

.spacer-bottom {
  height: 80px;
}

.loading-text {
  color: #888;
  font-size: 14px;
  flex: none !important; /* 避免 loading 文字被拉伸 */
}
</style>
