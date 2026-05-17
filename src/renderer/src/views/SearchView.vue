<script setup lang="ts">
import { usePlayerStore } from '@renderer/stores/playerStore'
import { SongDetailResult } from '@renderer/types/songDetail'
import { computed, ref, onMounted, onUnmounted, watch } from 'vue'
import { PlaylistCatlist, PlaylistCategory } from '@renderer/types/playlistCatlist'
import type { SearchResult, Song as SearchSong } from '@renderer/types/search'
import { resolveCachedMediaUrl } from '@renderer/utils/cache'
import { useFavoriteStore } from '@renderer/stores/favoriteStore'
import { CurrentSong, createCurrentSongArtists } from '@renderer/stores/playerStore'

const searchQuery = ref('')
const searchResults = ref<SearchResult | null>(null)
const isSearching = ref(false)
const isLoadingMore = ref(false)
const hasMore = ref(false)
const offset = ref(0)
const limit = 30
const coverMap = ref<Record<number, string>>({})

// --- 滚动逻辑控制 ---
const scrollContainer = ref<HTMLElement | null>(null)
const bottomObserver = ref<HTMLElement | null>(null)
const isFloating = ref(false) // 是否处于悬浮气泡状态
const lastScrollTop = ref(0) // 记录上次滚动位置

const hideProgress = ref(0) // 0 = 完全显示，1 = 完全划走

const handleScroll = (): void => {
  if (!scrollContainer.value) return
  const cur = scrollContainer.value.scrollTop
  const delta = cur - lastScrollTop.value

  // 向下滚：增加 progress，向上滚：减少
  hideProgress.value = Math.min(
    1,
    Math.max(0, hideProgress.value + delta / 120) // 120 是“手感系数”，不是高度
  )

  isFloating.value = cur > 60
  lastScrollTop.value = cur
}

// --- 原有逻辑 ---
interface DisplayCategory extends PlaylistCategory {
  color: string
}
const browseCategories = ref<DisplayCategory[]>([])
const gradientColors = [
  'bg-gradient-purple',
  'bg-gradient-orange',
  'bg-gradient-red',
  'bg-gradient-blue',
  'bg-gradient-green',
  'bg-gradient-dark',
  'bg-gradient-pink',
  'bg-gradient-indigo'
]

const getBrowseCategories = async (): Promise<void> => {
  try {
    const res = (await window.api.playlist_catlist({})) as { body?: PlaylistCatlist }
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

// Intersection Observer for infinite scroll
let observer: IntersectionObserver | null = null

onMounted(() => {
  getBrowseCategories()
  scrollContainer.value?.addEventListener('scroll', handleScroll, { passive: true })

  observer = new IntersectionObserver(
    (entries) => {
      if (
        entries[0].isIntersecting &&
        hasMore.value &&
        !isLoadingMore.value &&
        !isSearching.value
      ) {
        loadMore()
      }
    },
    {
      root: scrollContainer.value,
      threshold: 0.1
    }
  )
})

watch(bottomObserver, (newEl) => {
  if (observer) {
    observer.disconnect()
    if (newEl) {
      observer.observe(newEl)
    }
  }
})

onUnmounted(() => {
  scrollContainer.value?.removeEventListener('scroll', handleScroll)
  if (observer) {
    observer.disconnect()
  }
})

const hasSearched = computed(
  () => searchQuery.value.trim().length > 0 && searchResults.value !== null
)

const loadCover = async (id: number): Promise<void> => {
  if (coverMap.value[id]) return
  const res = (await window.api.song_detail({ ids: [id] })) as { body?: SongDetailResult }
  const url = res.body?.songs?.[0]?.al?.picUrl
  if (url) coverMap.value[id] = await resolveCachedMediaUrl(`${url}?param=80y80`)
}

const showResults = computed(() => hasSearched.value)

const handleSearch = async (): Promise<void> => {
  const kw = searchQuery.value.trim()
  if (!kw) {
    searchResults.value = null
    return
  }
  isSearching.value = true
  offset.value = 0
  try {
    const res = (await window.api.search({
      keywords: kw,
      limit,
      offset: offset.value
    })) as { body?: { result?: SearchResult } }
    if (res.body?.result?.songs) {
      searchResults.value = res.body.result
      const count = res.body.result.songCount || 0
      hasMore.value = res.body.result.songs.length + offset.value < count
      res.body.result.songs.forEach((song) => loadCover(song.id))
    } else {
      searchResults.value = null
      hasMore.value = false
    }
  } catch (err) {
    console.error('搜索失败', err)
    searchResults.value = null
    hasMore.value = false
  } finally {
    isSearching.value = false
  }
}

const loadMore = async (): Promise<void> => {
  const kw = searchQuery.value.trim()
  if (!kw || isLoadingMore.value || !hasMore.value) return

  isLoadingMore.value = true
  offset.value += limit

  try {
    const res = (await window.api.search({
      keywords: kw,
      limit,
      offset: offset.value
    })) as { body?: { result?: SearchResult } }

    if (res.body?.result?.songs) {
      if (searchResults.value) {
        searchResults.value.songs.push(...res.body.result.songs)
        const totalLoaded = searchResults.value.songs.length
        const totalAvailable = res.body.result.songCount || searchResults.value.songCount || 0
        hasMore.value = totalLoaded < totalAvailable
      } else {
        searchResults.value = res.body.result
        hasMore.value =
          res.body.result.songs.length + offset.value < (res.body.result.songCount || 0)
      }
      res.body.result.songs.forEach((song) => loadCover(song.id))
    } else {
      hasMore.value = false
    }
  } catch (err) {
    console.error('加载更多失败', err)
  } finally {
    isLoadingMore.value = false
  }
}

const clearSearch = (): void => {
  searchQuery.value = ''
  searchResults.value = null
}

const playerStore = usePlayerStore()
const favoriteStore = useFavoriteStore()
const playSong = (song: SearchSong): void => {
  void playerStore.playMusic(song.id)
}

const mapSearchSongToCurrentSong = (song: SearchSong): CurrentSong => ({
  id: song.id,
  name: song.name,
  artists: createCurrentSongArtists(song.artists),
  cover: coverMap.value[song.id] || '',
  duration: song.duration
})

const toggleFavorite = (song: SearchSong): void => {
  void favoriteStore.toggleFavorite(mapSearchSongToCurrentSong(song))
}
</script>

<template>
  <!-- 注意：监听此容器的滚动 -->
  <main ref="scrollContainer" class="scrollable-content search-container">
    <!-- === 头部搜索栏容器 === -->
    <!-- 动态类：is-floating(气泡化), is-hidden(向下滚动时移出) -->
    <header
      class="search-header"
      :class="{ 'is-floating': isFloating }"
      :style="{ '--hide-progress': hideProgress }"
    >
      <h1 class="page-title">搜索</h1>

      <div class="search-bar-wrapper">
        <svg
          class="search-icon"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
        </svg>
        <input
          v-model="searchQuery"
          type="text"
          placeholder="搜索歌曲、 艺人、专辑、歌词、歌单"
          class="search-input"
          @keyup.enter="handleSearch"
        />
        <button v-if="searchQuery" class="clear-btn" @click="clearSearch">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>
    </header>

    <!-- 占位符：防止 Header 变为 fixed 时内容闪跳 -->
    <div v-if="isFloating" class="header-placeholder"></div>

    <!-- === 状态 1: 浏览分类 === -->
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
      <div v-if="isSearching" class="loading">Searching...</div>

      <section class="section">
        <div class="section-header">
          <h2>
            歌曲 <span class="count">({{ searchResults?.songCount || 0 }})</span>
          </h2>
        </div>
        <div class="songs-list">
          <div
            v-for="song in searchResults?.songs"
            :key="song.id"
            class="song-row"
            @click="playSong(song)"
          >
            <div class="song-left">
              <img :src="coverMap[song.id]" class="song-cover" alt="cover" />
              <div class="song-details">
                <div class="song-title">{{ song.name }}</div>
                <div class="song-artist">
                  {{ song.artists.map((artist) => artist.name).join(', ') }}
                  <span v-if="song.album.name"> · {{ song.album.name }}</span>
                </div>
              </div>
            </div>
            <div class="song-right">
              <button
                class="favorite-btn"
                :class="{ active: favoriteStore.isFavorite(song.id) }"
                :title="favoriteStore.isFavorite(song.id) ? '取消喜欢' : '喜欢'"
                @click.stop="toggleFavorite(song)"
              >
                <svg viewBox="0 0 24 24" width="16" height="16">
                  <path
                    d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7 7-7Z"
                  />
                </svg>
              </button>
              <span class="song-duration">
                {{ Math.floor(song.duration / 60000) }}:{{
                  String(Math.floor((song.duration % 60000) / 1000)).padStart(2, '0')
                }}
              </span>
              <button class="more-btn">•••</button>
            </div>
          </div>
          <div v-if="searchResults?.songs.length === 0" class="empty-tips">No songs found</div>
        </div>

        <!-- 底部加载更多观测点 -->
        <div ref="bottomObserver" class="bottom-observer">
          <div v-if="isLoadingMore" class="loading-more">
            <div class="spinner"></div>
            <span>正在加载更多...</span>
          </div>
          <div v-else-if="!hasMore && searchResults?.songs.length" class="no-more">没有更多了</div>
        </div>
      </section>

      <div class="spacer-bottom"></div>
    </div>
  </main>
</template>

<style scoped>
* {
  box-sizing: border-box;
  font-family: 'Inter', sans-serif;
}

/* 容器布局 */
.search-container {
  height: 100%;
  overflow-y: auto;
  padding: 0 24px;
  scroll-behavior: smooth;
  position: relative;
}

/* === 核心：Search Header 样式 === */
.search-header {
  position: sticky;
  top: 0;
  transform: translateY(calc(-100% * var(--hide-progress)));
  transition: transform 0.12s linear;
}

/* 标题样式：通过透明度和高度控制隐藏 */
.page-title {
  margin-top: 10px;
  font-size: 34px;
  font-weight: 800;
  margin-bottom: 20px;
  color: var(--sys-text);
  transition: all 0.3s ease;
  opacity: 1;
  transform: translateY(0);
}

/* 搜索框包装器 */
.search-bar-wrapper {
  position: relative;
  display: flex;
  align-items: center;
  width: 100%;
  max-width: 600px;
  transition: all 0.4s ease;
}

/* 搜索输入框默认样式 */
.search-input {
  width: 100%;
  height: 48px;
  padding: 0 44px;
  border-radius: 14px;
  border: 1px solid var(--sys-border);
  background-color: var(--sys-surface-strong);
  font-size: 16px;
  color: var(--sys-text);
  outline: none;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.04);
  transition: all 0.3s ease;
}

/* === 关键状态：浮动气泡（液态玻璃） === */
.search-header.is-floating {
  padding: 10px 0; /* 压缩容器高度 */
}

.search-header.is-floating .page-title {
  opacity: 0;
  height: 0;
  margin: 0;
  pointer-events: none;
  transform: translateY(-10px);
}

.search-header.is-floating .search-bar-wrapper {
  max-width: 500px; /* 气泡变窄 */
  margin: 0 auto; /* 居中 */
}

.search-header.is-floating .search-input {
  /* 液态玻璃效果 */
  background: var(--sys-surface);
  backdrop-filter: var(--sys-glass-blur);
  -webkit-backdrop-filter: var(--sys-glass-blur);
  border: 1px solid var(--sys-border);
  box-shadow: var(--sys-shadow-soft);
  border-radius: 24px; /* 更加圆润 */
}

/* 占位符防止抖动 */
.header-placeholder {
  height: 130px; /* 大约等于标题+输入框的高度 */
}

/* 基础元素样式维持原样 */
.search-icon {
  position: absolute;
  left: 16px;
  width: 18px;
  height: 18px;
  color: var(--sys-text-tertiary);
  z-index: 1;
}

.clear-btn {
  position: absolute;
  right: 12px;
  background: none;
  border: none;
  color: var(--sys-text-tertiary);
  cursor: pointer;
  padding: 4px;
}

.spacer-bottom {
  height: 120px;
}

/* 分类卡片与列表样式 */
.section-header {
  margin: 24px 0 16px;
}
.section-header h2 {
  font-size: 20px;
  font-weight: 700;
  color: var(--sys-text);
}

.categories-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 16px;
}

.category-card {
  height: 100px;
  border-radius: 12px;
  padding: 16px;
  position: relative;
  overflow: hidden;
  cursor: pointer;
  transition: transform 0.2s;
}
.category-card:hover {
  transform: scale(1.02);
}
.category-name {
  font-size: 18px;
  font-weight: 700;
  color: var(--sys-on-accent);
  position: relative;
  z-index: 2;
}
.card-decoration {
  position: absolute;
  right: -10px;
  bottom: -10px;
  width: 60px;
  height: 60px;
  background: rgba(255, 255, 255, 0.2);
  transform: rotate(25deg);
  border-radius: 8px;
}

/* 渐变色 */
.bg-gradient-purple {
  background: linear-gradient(135deg, var(--theme-color), var(--theme-color-strong));
}
.bg-gradient-orange {
  background: linear-gradient(135deg, var(--sys-category-warm), var(--sys-category-sun));
}
.bg-gradient-red {
  background: linear-gradient(135deg, var(--sys-danger), var(--sys-category-red));
}
.bg-gradient-blue {
  background: linear-gradient(135deg, var(--sys-category-cyan), var(--sys-category-sky));
}
.bg-gradient-green {
  background: linear-gradient(135deg, var(--sys-success), var(--sys-category-mint));
}
.bg-gradient-dark {
  background: linear-gradient(135deg, var(--sys-bg-muted), var(--sys-text-secondary));
}
.bg-gradient-pink {
  background: linear-gradient(135deg, var(--theme-color-strong), var(--sys-category-rose));
}
.bg-gradient-indigo {
  background: linear-gradient(135deg, var(--theme-color), var(--sys-category-ink));
}

.songs-list {
  background: var(--sys-surface-muted);
  border-radius: 16px;
}
.song-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 16px;
  transition: background 0.2s;
  cursor: pointer;
}
.song-row:hover {
  background: var(--sys-control-hover);
}
.song-left {
  display: flex;
  align-items: center;
  gap: 12px;
}
.song-cover {
  width: 44px;
  height: 44px;
  border-radius: 8px;
  object-fit: cover;
}
.song-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--sys-text);
}
.song-artist {
  font-size: 12px;
  color: var(--sys-text-secondary);
}
.song-duration {
  font-size: 13px;
  color: var(--sys-text-tertiary);
}
.song-right {
  display: flex;
  align-items: center;
  gap: 10px;
}
.favorite-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--sys-text-disabled);
  cursor: pointer;
  opacity: 0;
  transition:
    color 0.2s,
    opacity 0.2s,
    background 0.2s;
}
.favorite-btn svg {
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.favorite-btn:hover {
  background: var(--sys-control-hover);
  color: var(--theme-color-strong);
}
.favorite-btn.active {
  color: var(--theme-color-strong);
  opacity: 1;
}
.favorite-btn.active svg {
  fill: currentColor;
}
.song-row:hover .favorite-btn {
  opacity: 1;
}
.more-btn {
  border: none;
  background: none;
  color: var(--sys-text-tertiary);
  cursor: pointer;
}

.fade-in {
  animation: fadeIn 0.4s ease-out;
}
@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.loading,
.empty-tips {
  text-align: center;
  padding: 40px;
  color: var(--sys-text-tertiary);
}

.bottom-observer {
  padding: 24px 0;
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 60px;
}

.loading-more {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--sys-text-tertiary);
  font-size: 14px;
}

.spinner {
  width: 18px;
  height: 18px;
  border: 2px solid var(--sys-control-hover);
  border-top-color: var(--theme-color);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.no-more {
  color: var(--sys-text-disabled);
  font-size: 13px;
  letter-spacing: 1px;
}
</style>
