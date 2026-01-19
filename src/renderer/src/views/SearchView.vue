<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import { SongDetailResult } from '@renderer/types/songDetail'
import { computed, ref, onMounted } from 'vue'
import { PlaylistCatlist, PlaylistCategory } from '@renderer/types/playlistCatlist'

const searchQuery = ref('')
const searchResults = ref<SearchResult | null>(null)
const isSearching = ref(false)
const coverMap = ref<Record<number, string>>({})

interface DisplayCategory extends PlaylistCategory {
  color: string
}
const browseCategories = ref<DisplayCategory[]>([])

// 预设的渐变色数组，用于分配给分类卡片
const gradientColors = [
  'bg-gradient-purple', 'bg-gradient-orange', 'bg-gradient-red',
  'bg-gradient-blue', 'bg-gradient-green', 'bg-gradient-dark',
  'bg-gradient-pink', 'bg-gradient-indigo'
]

const getBrowseCategories = async (): Promise<void> => {
  try {
    const res = await window.api.playlist_catlist({}) as { body?: PlaylistCatlist }

    if (res.body && res.body.sub) {
      browseCategories.value = res.body.sub.slice(0, 12).map((item, index) => ({
        ...item,
        color: gradientColors[index % gradientColors.length]
      }))
    }
  } catch (err) {
    console.error('获取分类失败:', err)
  }
}

onMounted(() => {
  getBrowseCategories()
})

const hasSearched = computed(() =>
  searchQuery.value.trim().length > 0 && searchResults.value !== null
)

const loadCover = async (id: number) => {
  if (coverMap.value[id]) return
  const res = await window.api.song_detail({ ids: [id] }) as { body?: SongDetailResult }
  const url = res.body?.songs?.[0]?.al?.picUrl
  if (url) coverMap.value[id] = url
}

const showResults = computed(() => hasSearched.value)

const handleSearch = async (): Promise<void> => {
  const kw = searchQuery.value.trim()
  if (!kw) {
    searchResults.value = null
    return
  }

  isSearching.value = true
  try {
    const res = await window.api.search({
      keywords: kw,
      limit: 20,
    }) as { body?: { result?: SearchResult } }

    if (res.body?.result?.songs) {
      searchResults.value = res.body.result
      res.body.result.songs.forEach((s: any) => loadCover(s.id))
    } else {
      searchResults.value = null
    }
  } catch (err) {
    console.error('搜索失败', err)
    searchResults.value = null
  } finally {
    isSearching.value = false
  }
}

const clearSearch = (): void => {
  searchQuery.value = ''
  searchResults.value = null
}

const playerStore = usePlayerStore()

/**
 * 播放歌曲的处理函数
 */
const playSong = (song: Song) => {
  playerStore.playMusic(song.id)
}
</script>

<template>
  <main class="scrollable-content search-container">
    <!-- === 头部搜索栏 === -->
    <header class="search-header">
      <h1 class="page-title">搜索</h1>
      <div class="search-bar-wrapper">
        <svg class="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
        </svg>
        <input
          v-model="searchQuery"
          type="text"
          placeholder="搜索歌曲、 艺人、专辑、歌词、歌单"
          class="search-input"
          @keyup.enter="handleSearch"
        >
        <button v-if="searchQuery" @click="clearSearch" class="clear-btn">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>
    </header>

    <!-- === 状态 1: 浏览分类 (默认视图) === -->
    <section v-if="!showResults" class="browse-section fade-in">
      <div class="section-header">
        <h2>专区</h2>
      </div>
      <div class="categories-grid">
        <div
          v-for="cat in browseCategories"
          :key="cat.name"
          class="category-card"
          :class="cat.color"
        >
          <span class="category-name">{{ cat.name }}</span>
          <div class="card-decoration"></div>
        </div>
      </div>
    </section>

    <!-- === 状态 2: 搜索结果 === -->
    <div v-else class="results-section fade-in">
      <!-- 可选：加载状态 -->
      <div v-if="isSearching" class="loading">Searching...</div>

      <!-- 歌曲列表（真实数据） -->
      <section class="section">
        <div class="section-header">
          <h2>歌曲 <span class="count">({{ searchResults?.songCount || 0 }})</span></h2>
        </div>
        <div class="songs-list">
          <div
            v-for="song in searchResults?.songs"
            :key="song.id"
            class="song-row"
            @click="playSong(song)"
          >
            <div class="song-left">
              <img
                :src="coverMap[song.id]"
                class="song-cover"
                alt="cover"
              />
              <div class="song-details">
                <div class="song-title">{{ song.name }}</div>
                <div class="song-artist">
                  {{ song.artists.map((a: any) => a.name).join(', ') }}
                  <span v-if="song.album.name"> · {{ song.album.name }}</span>
                </div>
              </div>
            </div>
            <div class="song-right">
              <span class="song-duration">
                {{ Math.floor(song.duration / 60000) }}:{{ String(Math.floor((song.duration % 60000) / 1000)).padStart(2, '0') }}
              </span>
              <button class="more-btn">•••</button>
            </div>
          </div>

          <!-- 空结果提示 -->
          <div v-if="searchResults?.songs.length === 0" class="empty-tips">
            No songs found
          </div>
        </div>
      </section>

      <!-- 底部留白 -->
      <div class="spacer-bottom"></div>
    </div>
  </main>
</template>

<style scoped>
/* 继承原设计的基础字体设置 */
* { box-sizing: border-box; font-family: 'Inter', sans-serif; }

/* 容器布局 */
.search-container {
  flex: 1;
  overflow-y: auto;
  padding: 20px 24px 0 24px;
  /* 隐藏滚动条但保留功能 */
  scrollbar-width: none;
}
.search-container::-webkit-scrollbar { display: none; }

.page-title {
  font-size: 34px;
  font-weight: 800;
  margin-bottom: 20px;
  color: #111;
}

.spacer-bottom { height: 120px; }

/* === 搜索栏样式 === */
.search-header {
  margin-bottom: 30px;
  position: sticky;
  top: 0;
  z-index: 10;
  background: transparent; /* 背景由父级决定，或者是毛玻璃 */
}

.search-bar-wrapper {
  position: relative;
  display: flex;
  align-items: center;
  width: 100%;
  max-width: 600px;
}

.search-icon {
  position: absolute;
  left: 16px;
  width: 20px;
  height: 20px;
  color: #888;
  pointer-events: none;
}

.search-input {
  width: 100%;
  height: 48px;
  padding: 0 44px; /* 为图标留出空间 */
  border-radius: 12px;
  border: 1px solid rgba(0,0,0,0.05);
  background-color: #fff;
  font-size: 16px;
  color: #111;
  outline: none;
  box-shadow: 0 4px 12px rgba(0,0,0,0.03);
  transition: all 0.2s;
}

.search-input:focus {
  background-color: #fff;
  box-shadow: 0 4px 16px rgba(0,0,0,0.08);
  border-color: rgba(0,0,0,0.1);
}

.search-input::placeholder {
  color: #999;
}

.clear-btn {
  position: absolute;
  right: 12px;
  background: none;
  border: none;
  color: #999;
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
}
.clear-btn:hover { color: #333; }
.clear-btn svg { width: 18px; height: 18px; }

/* === 分类卡片样式 === */
.section-header { margin-bottom: 16px; }
.section-header h2 { font-size: 20px; font-weight: 700; color: #111; }

.categories-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 16px;
  padding-bottom: 120px;
}

.category-card {
  height: 100px;
  border-radius: 12px;
  padding: 16px;
  position: relative;
  overflow: hidden;
  cursor: pointer;
  transition: transform 0.2s;
  box-shadow: 0 4px 10px rgba(0,0,0,0.05);
}

.category-card:hover { transform: scale(1.02); }

.category-name {
  font-size: 18px;
  font-weight: 700;
  color: white;
  position: relative;
  z-index: 2;
}

/* 装饰性斜角 */
.card-decoration {
  position: absolute;
  right: -10px;
  bottom: -10px;
  width: 60px;
  height: 60px;
  background: rgba(255,255,255,0.2);
  transform: rotate(25deg);
  border-radius: 8px;
}

/* 渐变色背景 */
.bg-gradient-purple { background: linear-gradient(135deg, #8e2de2, #4a00e0); }
.bg-gradient-orange { background: linear-gradient(135deg, #f12711, #f5af19); }
.bg-gradient-red { background: linear-gradient(135deg, #cb2d3e, #ef473a); }
.bg-gradient-blue { background: linear-gradient(135deg, #2193b0, #6dd5ed); }
.bg-gradient-green { background: linear-gradient(135deg, #11998e, #38ef7d); }
.bg-gradient-dark { background: linear-gradient(135deg, #232526, #414345); }
.bg-gradient-pink { background: linear-gradient(135deg, #ec008c, #fc6767); }
.bg-gradient-indigo { background: linear-gradient(135deg, #4b6cb7, #182848); }

/* === 搜索结果 - 顶部结果 === */
.top-result-card {
  display: flex;
  align-items: center;
  background: white;
  padding: 16px;
  border-radius: 16px;
  gap: 16px;
  box-shadow: 0 4px 20px rgba(0,0,0,0.04);
  position: relative;
  transition: transform 0.2s;
}
.top-result-card:hover { background: #fcfcfc; }

.top-result-img {
  width: 80px;
  height: 80px;
  border-radius: 50%;
  object-fit: cover;
  box-shadow: 0 4px 10px rgba(0,0,0,0.1);
}

.top-result-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.result-title { font-size: 22px; font-weight: 700; margin: 0; color: #111; }
.result-meta { display: flex; gap: 8px; align-items: center; }
.tag-badge {
  background: #111; color: #fff; padding: 2px 8px;
  border-radius: 12px; font-size: 11px; font-weight: 600;
}
.tags { font-size: 13px; color: #666; }

.play-btn-circle.small {
  margin-left: auto;
  width: 36px; height: 36px;
  background: #fa233b; /* 强调色 */
  color: white;
  border: none;
  border-radius: 50%;
  display: flex; align-items: center; justify-content: center;
  cursor: pointer;
  box-shadow: 0 4px 10px rgba(250, 35, 59, 0.3);
}

/* === 搜索结果 - 歌曲列表 === */
.songs-list {
  background: rgba(255,255,255,0.5);
  border-radius: 16px;
  overflow: hidden;
}

.song-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 16px;
  transition: background 0.2s;
  cursor: pointer;
}

.song-row:hover { background: rgba(255,255,255,0.8); }

.song-left { display: flex; align-items: center; gap: 12px; }
.song-cover { width: 40px; height: 40px; border-radius: 6px; }
.song-details { display: flex; flex-direction: column; }
.song-title { font-size: 14px; font-weight: 600; color: #111; }
.song-artist { font-size: 12px; color: #666; }

.song-right { display: flex; align-items: center; gap: 16px; }
.song-duration { font-size: 13px; color: #888; font-variant-numeric: tabular-nums; }
.more-btn { border: none; background: none; color: #999; cursor: pointer; padding: 4px; font-size: 12px; }
.more-btn:hover { color: #333; }

/* === 搜索结果 - 艺人列表 === */
.artists-row {
  display: flex;
  gap: 20px;
  overflow-x: auto;
  padding-bottom: 10px;
}
.artist-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.artist-avatar {
  width: 100px; height: 100px;
  border-radius: 50%;
  object-fit: cover;
  box-shadow: 0 4px 12px rgba(0,0,0,0.1);
  transition: transform 0.2s;
}
.artist-item:hover .artist-avatar { transform: scale(1.05); }
.artist-name { font-size: 13px; font-weight: 600; color: #333; }

/* 简单的淡入动画 */
.fade-in {
  animation: fadeIn 0.3s ease-in-out;
}
@keyframes fadeIn {
  from { opacity: 0; transform: translateY(10px); }
  to { opacity: 1; transform: translateY(0); }
}

.count { font-size: 14px; color: #888; font-weight: normal; }
.loading {
  text-align: center;
  padding: 40px;
  color: #666;
  font-size: 16px;
}
.empty-tips {
  text-align: center;
  padding: 40px;
  color: #999;
  font-size: 15px;
}
</style>
