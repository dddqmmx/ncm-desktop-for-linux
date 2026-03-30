<template>
  <div class="artist-page">
    <div class="scroll-container">
      <main class="main-content">
        <div v-if="loading" class="state-card">
          <div class="spinner" aria-hidden="true"></div>
          <p class="state-title">正在加载歌手信息</p>
          <p class="state-desc">正在获取热门歌曲、专辑和 MV 数据。</p>
        </div>

        <div v-else-if="errorMessage" class="state-card">
          <p class="state-title">页面加载失败</p>
          <p class="state-desc">{{ errorMessage }}</p>
          <button class="btn-play" type="button" @click="retryFetch">重试</button>
        </div>

        <template v-else>
          <!-- ───── ARTIST HEADER ───── -->
          <header class="artist-header">
            <div class="artist-avatar-col">
              <div class="avatar-ring-wrapper">
                <UserAvatar
                  :id="artistProfile?.avatar || artistProfile?.cover"
                  :alt="artistProfile?.name"
                  size="400y400"
                />
              </div>
              <div class="avatar-glow" aria-hidden="true"></div>
            </div>

            <div class="artist-body">
              <h1 class="artist-name">{{ artist.name }}</h1>

              <div class="pill-row">
                <span v-for="pill in artistPills" :key="pill" class="pill">{{ pill }}</span>
              </div>

              <p class="artist-summary">{{ artistSummary }}</p>

              <div class="cta-row">
                <button
                  class="btn-play"
                  type="button"
                  :disabled="!topSongs.length"
                  @click="handlePlayAll"
                >
                  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
                    <path d="M7.5 5.5v13l10-6.5-10-6.5Z" fill="currentColor" />
                  </svg>
                  播放
                </button>
                <button
                  class="btn-shuffle"
                  type="button"
                  :disabled="!topSongs.length"
                  @click="handleShufflePlay"
                >
                  <svg
                    viewBox="0 0 24 24"
                    width="18"
                    height="18"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <g>
                      <path d="M16 3h5v5" />
                      <path d="M4 20L21 3" />
                      <path d="M21 16v5h-5" />
                      <path d="M15 15l6 6" />
                      <path d="M4 4l5 5" />
                    </g>
                  </svg>
                  随机播放
                </button>
              </div>
            </div>
          </header>

          <!-- ───── LATEST RELEASES ───── -->
          <section v-if="latestReleases.length">
            <div class="section-header">
              <div>
                <p class="eyebrow-sm">Latest</p>
                <h2 class="section-title">最新发布</h2>
              </div>
            </div>
            <div class="latest-grid">
              <div v-for="item in latestReleases" :key="item.id" class="latest-item">
                <div class="latest-img-wrap-wrapper">
                  <AlbumCover :id="item.cover" :alt="item.title" size="200y200" />
                </div>

                <div class="latest-info">
                  <p class="tile-type">{{ item.type }}</p>
                  <h4 class="tile-name">{{ item.title }}</h4>
                  <p class="tile-year">{{ item.year }}</p>
                </div>
              </div>
            </div>
          </section>

          <!-- ───── TOP SONGS ───── -->
          <section v-if="topSongs.length">
            <div class="section-header">
              <div>
                <p class="eyebrow-sm">Top Tracks</p>
                <h2 class="section-title">热门歌曲</h2>
              </div>
              <span class="count-badge">{{ topSongs.length }} 首</span>
            </div>

            <div class="card glass-card songs-card">
              <div
                v-for="(song, i) in topSongs"
                :key="song.id"
                class="song-row"
                @click="handlePlaySong(song.id)"
              >
                <span class="song-num">{{ formatIndex(i + 1) }}</span>
                <div class="song-thumb-wrapper">
                  <SongCover :id="song.cover" :alt="song.title" size="112y112" />
                </div>
                <div class="song-info">
                  <div class="song-name-row">
                    <span class="song-name">{{ song.title }}</span>
                    <span
                      v-if="i < 2"
                      class="song-badge"
                      :class="i === 0 ? 'badge-hot' : 'badge-rec'"
                    >
                      {{ i === 0 ? '最热' : '推荐' }}
                    </span>
                  </div>
                  <span class="song-album">{{ song.album }}</span>
                </div>
                <span class="song-dur">{{ song.duration }}</span>
              </div>
            </div>
          </section>

          <!-- ───── ALBUMS ───── -->
          <section v-if="albums.length">
            <div class="section-header">
              <div>
                <p class="eyebrow-sm">Discography</p>
                <h2 class="section-title">专辑</h2>
              </div>
              <span class="count-badge">{{ albums.length }} 张</span>
            </div>
            <div class="media-grid">
              <div
                v-for="album in albums"
                :key="album.id"
                class="media-tile"
                @click="$router.push('/album/' + album.id)"
              >
                <div class="tile-img-wrap-wrapper">
                  <AlbumCover :id="album.cover" :alt="album.title" size="400y400" />
                </div>
                <p class="tile-type">Album</p>
                <h4 class="tile-name">{{ album.title }}</h4>
                <p class="tile-year">{{ album.year }}</p>
              </div>
            </div>
          </section>

          <!-- ───── EPS & SINGLES ───── -->
          <section v-if="eps.length">
            <div class="section-header">
              <div>
                <p class="eyebrow-sm">Singles</p>
                <h2 class="section-title">EP 和单曲</h2>
              </div>
              <span class="count-badge">{{ eps.length }} 张</span>
            </div>
            <div class="media-grid">
              <div
                v-for="ep in eps"
                :key="ep.id"
                class="media-tile"
                @click="$router.push('/album/' + ep.id)"
              >
                <div class="tile-img-wrap-wrapper">
                  <AlbumCover :id="ep.cover" :alt="ep.title" size="400y400" />
                </div>
                <p class="tile-type">EP / Single</p>
                <h4 class="tile-name">{{ ep.title }}</h4>
                <p class="tile-year">{{ ep.year }}</p>
              </div>
            </div>
          </section>

          <!-- ───── MUSIC VIDEOS ───── -->
          <section v-if="mvs.length" class="pb-safe">
            <div class="section-header">
              <div>
                <h2 class="section-title">MV</h2>
              </div>
              <span class="count-badge">{{ mvs.length }} 部</span>
            </div>

            <div class="media-grid mv-grid">
              <div v-for="mv in mvs" :key="mv.id" class="media-tile mv-tile">
                <div class="tile-img-wrap-wrapper mv-img-wrap-wrapper">
                  <LazyImage :id="mv.cover" :alt="mv.title" param="540y304" />
                  <div class="mv-play-btn" aria-hidden="true">
                    <svg viewBox="0 0 24 24" width="22" height="22">
                      <path d="M8 6.5v11l9-5.5-9-5.5Z" fill="currentColor" />
                    </svg>
                  </div>
                </div>
                <p class="tile-type">Music Video</p>
                <h4 class="tile-name">{{ mv.title }}</h4>
                <p class="tile-year">{{ mv.year }}</p>
              </div>
            </div>
          </section>
        </template>
      </main>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { CurrentSong, createCurrentSongArtists, usePlayerStore } from '@renderer/stores/playerStore'
import type {
  ArtistAlbum,
  ArtistAlbumResponse,
  ArtistDetailResponse,
  ArtistMv,
  ArtistMvResponse,
  ArtistProfile,
  ArtistSong,
  ArtistTopSongResponse
} from '@renderer/types/artist'
import AlbumCover from '../components/AlbumCover.vue'
import SongCover from '../components/SongCover.vue'
import UserAvatar from '../components/UserAvatar.vue'
import LazyImage from '../components/LazyImage.vue'

interface ServiceResponse<T> {
  body?: T | null
}

interface ReleaseItem {
  id: number
  type: string
  title: string
  year: string
  cover: string
}

interface SongItem {
  id: number
  title: string
  album: string
  duration: string
  cover: string
}

interface MediaItem {
  id: number
  title: string
  year: string
  cover: string
}

const FALLBACK_COVER = `data:image/svg+xml;charset=UTF-8,${encodeURIComponent(
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 240"><rect width="240" height="240" rx="28" fill="#f3f4f6"/><circle cx="120" cy="88" r="34" fill="#d1d5db"/><path d="M56 188c12-32 44-52 64-52s52 20 64 52" fill="#d1d5db"/></svg>'
)}`

const route = useRoute()
const playerStore = usePlayerStore()

const loading = ref(true)
const errorMessage = ref('')
const artistProfile = ref<ArtistProfile | null>(null)
const topSongData = ref<ArtistSong[]>([])
const albumData = ref<ArtistAlbum[]>([])
const mvData = ref<ArtistMv[]>([])

let requestSerial = 0

const formatIndex = (n: number): string => n.toString().padStart(2, '0')

const formatDuration = (ms: number): string => {
  const totalSeconds = Math.floor(ms / 1000)
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes}:${seconds.toString().padStart(2, '0')}`
}

const formatYear = (value?: number | string): string => {
  if (!value) return '--'
  if (typeof value === 'string') {
    const matchedYear = value.match(/\d{4}/)
    return matchedYear?.[0] ?? value
  }

  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? '--' : `${date.getFullYear()}`
}

const withImageParam = (url: string | undefined, size: string): string => {
  if (!url) return FALLBACK_COVER
  if (
    url.startsWith('ncm-cache:') ||
    url.startsWith('file:') ||
    url.startsWith('data:') ||
    url.startsWith('blob:')
  ) {
    return url
  }
  const separator = url.includes('?') ? '&' : '?'
  return `${url}${separator}param=${size}`
}

const isEpOrSingle = (album: ArtistAlbum): boolean => {
  const typeInfo = `${album.type ?? ''} ${album.subType ?? ''}`.toLowerCase()
  return /single|ep|单曲/.test(typeInfo)
}

const toCurrentSong = (song: ArtistSong): CurrentSong => ({
  id: song.id,
  name: song.name,
  artists: createCurrentSongArtists(song.ar),
  cover: withImageParam(song.al.picUrl, '200y200'),
  duration: song.dt
})

const toMediaItem = (album: ArtistAlbum): MediaItem => ({
  id: album.id,
  title: album.name,
  year: formatYear(album.publishTime),
  cover: withImageParam(album.picUrl, '400y400')
})

const sortedAlbums = computed(() =>
  [...albumData.value].sort((a, b) => (b.publishTime ?? 0) - (a.publishTime ?? 0))
)

const artist = computed(() => ({
  name: artistProfile.value?.name ?? '未知歌手',
  avatar: withImageParam(artistProfile.value?.avatar || artistProfile.value?.cover, '400y400')
}))

const latestReleases = computed<ReleaseItem[]>(() =>
  sortedAlbums.value.slice(0, 2).map((album) => ({
    ...toMediaItem(album),
    type: isEpOrSingle(album) ? 'Single / EP' : 'Album'
  }))
)

const topSongs = computed<SongItem[]>(() =>
  topSongData.value.map((song) => ({
    id: song.id,
    title: song.name,
    album: song.al.name,
    duration: formatDuration(song.dt),
    cover: withImageParam(song.al.picUrl, '112y112')
  }))
)

const albums = computed<MediaItem[]>(() =>
  sortedAlbums.value.filter((album) => !isEpOrSingle(album)).map(toMediaItem)
)

const eps = computed<MediaItem[]>(() => sortedAlbums.value.filter(isEpOrSingle).map(toMediaItem))

const mvs = computed<MediaItem[]>(() =>
  mvData.value.map((mv) => ({
    id: mv.id,
    title: mv.name,
    year: formatYear(mv.publishTime),
    cover: withImageParam(mv.imgurl16v9 || mv.imgurl, '540y304')
  }))
)

const artistPills = computed(() => [
  `${artistProfile.value?.albumSize ?? albums.value.length} 张专辑`,
  `${eps.value.length} 张 EP / 单曲`,
  `${artistProfile.value?.mvSize ?? mvs.value.length} 支 MV`
])

const artistSummary = computed(() => {
  const briefDesc = artistProfile.value?.briefDesc?.trim()
  if (briefDesc) return briefDesc

  const latestTitle = latestReleases.value[0]?.title
  if (latestTitle) {
    return `最新发布《${latestTitle}》，当前已收录 ${topSongs.value.length} 首热门歌曲。`
  }

  return `当前已收录 ${topSongs.value.length} 首热门歌曲、${albums.value.length} 张专辑。`
})

const songQueue = computed<CurrentSong[]>(() => topSongData.value.map(toCurrentSong))

const handlePlayAll = (): void => {
  if (!songQueue.value.length) return
  void playerStore.playAll(songQueue.value)
}

const handleShufflePlay = (): void => {
  if (!songQueue.value.length) return
  const startIndex = Math.floor(Math.random() * songQueue.value.length)
  void playerStore.playAll(songQueue.value, startIndex)
}

const handlePlaySong = (songId: number): void => {
  void playerStore.playMusic(songId)
}

const fetchArtistData = async (artistIdParam: string | string[] | undefined): Promise<void> => {
  const rawId = Array.isArray(artistIdParam) ? artistIdParam[0] : artistIdParam
  const artistId = Number(rawId)
  const currentRequest = ++requestSerial

  if (!Number.isFinite(artistId) || artistId <= 0) {
    artistProfile.value = null
    topSongData.value = []
    albumData.value = []
    mvData.value = []
    errorMessage.value = '歌手 ID 无效'
    loading.value = false
    return
  }

  loading.value = true
  errorMessage.value = ''

  try {
    const [detailRes, topSongRes, albumRes, mvRes] = (await Promise.all([
      window.api.artist_detail({ id: artistId }),
      window.api.artist_top_song({ id: artistId }),
      window.api.artist_album({ id: artistId, limit: 100, offset: 0 }),
      window.api.artist_mv({ id: artistId, limit: 50, offset: 0 })
    ])) as [
      ServiceResponse<ArtistDetailResponse>,
      ServiceResponse<ArtistTopSongResponse>,
      ServiceResponse<ArtistAlbumResponse>,
      ServiceResponse<ArtistMvResponse>
    ]

    if (currentRequest !== requestSerial) return

    artistProfile.value = detailRes.body?.data?.artist ?? null
    topSongData.value = topSongRes.body?.songs ?? []
    albumData.value = albumRes.body?.hotAlbums ?? []
    mvData.value = mvRes.body?.mvs ?? []

    if (!artistProfile.value) {
      errorMessage.value = '未获取到歌手详情'
    }
  } catch (error) {
    if (currentRequest !== requestSerial) return

    console.error('Failed to fetch artist data:', error)
    artistProfile.value = null
    topSongData.value = []
    albumData.value = []
    mvData.value = []
    errorMessage.value = '歌手信息加载失败，请稍后重试。'
  } finally {
    if (currentRequest === requestSerial) {
      loading.value = false
    }
  }
}

const retryFetch = (): void => {
  void fetchArtistData(route.params.id)
}

watch(
  () => route.params.id,
  (artistId) => {
    void fetchArtistData(artistId)
  },
  { immediate: true }
)
</script>

<style scoped>
.artist-page {
  /* 现代亮色背景与毛玻璃 */
  --glass: rgba(255, 255, 255, 0.65);
  --glass-border: rgba(255, 255, 255, 1);
  --glass-hover: rgba(255, 255, 255, 0.85);

  /* 字体颜色栈 */
  --text-1: #1d1d1f;
  --text-2: #86868b;
  --text-3: #a1a1a6;

  /* 雅致的阴影 */
  --shadow-card: 0 10px 40px -10px rgba(0, 0, 0, 0.05);
  --shadow-inset: inset 0 0 0 1px rgba(255, 255, 255, 0.8);

  /* 圆角 */
  --radius-card: 24px;
  --radius-img: 16px;

  height: 100%;
  min-height: 0;
  box-sizing: border-box;
  font-family:
    -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'SF Pro Display', 'Helvetica Neue', sans-serif;
  color: var(--text-1);
  overflow: hidden;
  -webkit-font-smoothing: antialiased;
}

/* ─── Scroll container ───────────────────────────────────── */
.scroll-container {
  position: relative;
  z-index: 1;
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-width: thin;
  scrollbar-color: rgba(29, 29, 31, 0.18) transparent;
  scrollbar-gutter: stable;
}
.scroll-container::-webkit-scrollbar {
  width: 8px;
}

.scroll-container::-webkit-scrollbar-track {
  background: transparent;
}

.scroll-container::-webkit-scrollbar-thumb {
  background: rgba(29, 29, 31, 0.18);
  border-radius: 999px;
}
.scroll-container::-webkit-scrollbar {
  display: none;
}

/* ─── Main layout ────────────────────────────────────────── */
.main-content {
  max-width: 1100px;
  margin: 0 auto;
  padding: 56px 32px 120px;
  display: flex;
  flex-direction: column;
  gap: 60px;
}

.state-card {
  min-height: 280px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 32px;
  text-align: center;
}

.state-title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
  color: var(--text-1);
}

.state-desc {
  max-width: 460px;
  margin: 0;
  line-height: 1.6;
  color: var(--text-2);
}

.spinner {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  border: 3px solid rgba(0, 0, 0, 0.08);
  border-top-color: rgba(0, 0, 0, 0.6);
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* ─── Artist Header (原 Hero) ────────────────────────────────── */
.artist-header {
  display: flex;
  align-items: flex-start;
  gap: 48px;
}

.artist-avatar-col {
  width: 200px;
  height: 200px;
  flex-shrink: 0;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}

.avatar-ring-wrapper {
  width: 100%;
  height: 100%;
  border-radius: 50%;
  overflow: hidden;
  transition: transform 0.3s ease;
}

.artist-avatar-col:hover .avatar-ring-wrapper {
  transform: scale(1.02);
}

.avatar-glow {
  position: absolute;
  inset: -20px;
  border-radius: 50%;
  background: radial-gradient(circle, rgba(29, 29, 31, 0.12) 0%, transparent 72%);
  pointer-events: none;
}

.artist-body {
  display: flex;
  flex-direction: column;
  justify-content: center; /* 头像与简介等高布局，内容在此垂直居中对齐 */
  flex: 1 1 0;
  min-height: 200px;
  gap: 16px;
  min-width: 0;
}

/* ─── Eyebrow / labels ───────────────────────────────────── */
.eyebrow,
.eyebrow-sm {
  margin: 0;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--text-2);
}
.eyebrow-sm {
  font-size: 10px;
}

/* ─── Artist name ────────────────────────────────────────── */
.artist-name {
  margin: 0;
  font-size: clamp(2.5rem, 4.5vw, 4.2rem);
  line-height: 1.05;
  letter-spacing: -0.04em;
  font-weight: 800;
  color: var(--text-1);
}

/* ─── Pills ──────────────────────────────────────────────── */
.pill-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.pill {
  padding: 6px 16px;
  border-radius: 999px;
  border: 1px solid rgba(0, 0, 0, 0.04);
  background: var(--glass);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.02);
  font-size: 13px;
  font-weight: 600;
  color: var(--text-2);
  backdrop-filter: blur(20px) saturate(180%);
}

/* ─── Artist summary ───────────────────────────────────────── */
.artist-summary {
  margin: 0;
  font-size: 15px;
  line-height: 1.6;
  color: var(--text-2);
  max-width: 680px;
  font-weight: 400;
}

/* ─── CTA buttons ────────────────────────────────────────── */
.cta-row {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  margin-top: 4px;
}

.btn-play,
.btn-shuffle {
  height: 44px;
  padding: 0 24px;
  border-radius: 22px;
  border: none;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.25, 1, 0.5, 1);
}
.btn-play:hover,
.btn-shuffle:hover {
  transform: scale(1.03);
}
.btn-play:active,
.btn-shuffle:active {
  transform: scale(0.97);
}
.btn-play:disabled,
.btn-shuffle:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  transform: none;
}

.btn-play {
  background: #000; /* 修改为黑色 */
  color: #fff;
  box-shadow: 0 8px 20px -6px rgba(0, 0, 0, 0.4);
}
.btn-shuffle {
  background: var(--glass);
  border: 1px solid rgba(0, 0, 0, 0.03);
  color: #000;
  backdrop-filter: blur(20px) saturate(180%);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.03);
}

/* ─── Glass card ─────────────────────────────────────────── */
.glass-card {
  background: var(--glass);
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-card);
  backdrop-filter: blur(40px) saturate(180%);
  box-shadow: var(--shadow-card), var(--shadow-inset);
}

/* ─── Section header ─────────────────────────────────────── */
.section-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 24px;
}
.section-title {
  margin: 6px 0 0;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.02em;
}
.count-badge {
  padding: 6px 14px;
  border-radius: 999px;
  background: var(--glass);
  border: 1px solid rgba(0, 0, 0, 0.03);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.02);
  font-size: 13px;
  font-weight: 600;
  color: var(--text-2);
  white-space: nowrap;
  backdrop-filter: blur(20px);
}

/* ─── Songs card ─────────────────────────────────────────── */
.songs-card {
  padding: 12px;
}

.song-row {
  display: grid;
  grid-template-columns: 40px 56px minmax(0, 1fr) auto;
  gap: 16px;
  align-items: center;
  padding: 10px 16px;
  border-radius: 14px;
  transition:
    background 0.2s ease,
    transform 0.2s ease;
  cursor: pointer;
}
.song-row + .song-row {
  margin-top: 2px;
}
.song-row:hover {
  background: var(--glass-hover);
  transform: scale(1.01);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.02);
}

.song-num {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
  text-align: right;
}
.song-thumb-wrapper {
  width: 56px;
  height: 56px;
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid rgba(0, 0, 0, 0.04);
}
.song-info {
  min-width: 0;
}
.song-name-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.song-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-1);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}
.song-badge {
  flex-shrink: 0;
  padding: 3px 8px;
  border-radius: 6px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.04em;
}
.badge-hot {
  background: #000;
  color: #fff;
}
.badge-rec {
  background: rgba(0, 0, 0, 0.05);
  color: var(--text-2);
}
.song-album {
  margin-top: 4px;
  font-size: 13px;
  font-weight: 400;
  color: var(--text-2);
}
.song-dur {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}

/* ─── 优雅的 Grid 铺展布局 (替代原本难用的水平滚动) ─── */
.media-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 24px;
}

.latest-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 24px;
}

.latest-item {
  display: flex;
  align-items: center;
  gap: 16px;
}

.latest-item:hover {
  transform: translateY(-2px);
}
.latest-img-wrap-wrapper {
  width: 80px;
  height: 80px;
  flex-shrink: 0;
  border-radius: 12px;
  overflow: hidden;
}

.latest-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.latest-item {
  backdrop-filter: blur(20px);
}

.tile-name {
  font-size: 15px;
  font-weight: 600;
}

.mv-grid {
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
}

/* ─── Media tile ─────────────────────────────────────────── */
.media-tile {
  min-width: 0;
  cursor: pointer;
}
.tile-img-wrap-wrapper {
  width: 100%;
  aspect-ratio: 1;
  border-radius: var(--radius-img);
  overflow: hidden;
  margin-bottom: 14px;
  border: 1px solid rgba(0, 0, 0, 0.04);
  box-shadow: 0 8px 24px -8px rgba(0, 0, 0, 0.1);
  position: relative;
  transition:
    transform 0.3s cubic-bezier(0.25, 1, 0.5, 1),
    box-shadow 0.3s ease;
}

.media-tile:hover .tile-img-wrap-wrapper {
  transform: translateY(-4px);
  box-shadow: 0 16px 32px -10px rgba(0, 0, 0, 0.15);
}

.tile-type {
  margin: 0 0 4px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-3);
}
.tile-name {
  margin: 0 0 4px;
  font-size: 15px;
  font-weight: 600;
  line-height: 1.3;
  color: var(--text-1);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.tile-year {
  margin: 0;
  font-size: 13px;
  color: var(--text-2);
}

/* ─── MV tile ────────────────────────────────────────────── */
.mv-img-wrap-wrapper {
  aspect-ratio: 16/9;
}
.mv-play-btn {
  position: absolute;
  bottom: 12px;
  left: 12px;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.85);
  backdrop-filter: blur(8px);
  color: #1d1d1f;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  opacity: 0;
  transform: scale(0.9);
  transition:
    opacity 0.3s ease,
    transform 0.3s cubic-bezier(0.25, 1, 0.5, 1);
}
.mv-tile:hover .mv-play-btn {
  opacity: 1;
  transform: scale(1);
}

.pb-safe {
  padding-bottom: 24px;
}

/* ─── Responsive ─────────────────────────────────────────── */
@media (max-width: 900px) {
  .main-content {
    padding: 40px 24px 100px;
    gap: 48px;
  }
  .artist-header {
    flex-direction: column;
    gap: 32px;
    text-align: center;
  }
  .artist-body {
    align-items: center;
    height: auto;
  }
  .pill-row,
  .cta-row {
    justify-content: center;
  }
  .artist-avatar-col {
    width: 180px;
    height: 180px;
  }
  .latest-grid {
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  }
}

@media (max-width: 640px) {
  .main-content {
    padding: 24px 16px 96px;
    gap: 40px;
  }
  .artist-name {
    font-size: 38px;
  }
  .cta-row {
    flex-direction: column;
    width: 100%;
  }
  .btn-play,
  .btn-shuffle {
    width: 100%;
    justify-content: center;
  }
  .section-header {
    align-items: flex-start;
    flex-direction: column;
    gap: 8px;
  }
  .song-row {
    grid-template-columns: 32px 48px minmax(0, 1fr);
    padding: 8px 12px;
  }
  .song-dur {
    display: none;
  }
  .media-grid,
  .latest-grid {
    grid-template-columns: repeat(2, minmax(140px, 1fr));
    gap: 16px;
  }
}
</style>
