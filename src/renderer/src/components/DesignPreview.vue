<script setup lang="ts">
import { ref } from 'vue'

// --- 模拟数据 ---

const navItems = [
  { id: 'search', name: 'Search', icon: 'search' },
  { id: 1, name: 'Home', active: true, icon: 'home' },
  { id: 2, name: 'New', icon: 'grid' },
  { id: 3, name: 'Radio', icon: 'radio' }
]

const libraryItems = [
  { id: 4, name: 'Pins', icon: 'pin' },
  { id: 5, name: 'Recently Added', icon: 'clock' },
  { id: 6, name: 'Artists', icon: 'mic' },
  { id: 7, name: 'Albums', icon: 'album' },
  { id: 8, name: 'Songs', icon: 'note' },
  { id: 9, name: 'Music Videos', icon: 'tv' },
  { id: 10, name: 'Made for You', icon: 'user' }
]

const topPicks = [
  {
    type: 'mix',
    title: 'Get Up!<br>Mix',
    desc: 'Bad Bunny, Kapo, Nao, Davido...',
    bgClass: 'bg-gradient-orange',
    image: null,
    logo: 'Music'
  },
  {
    type: 'show',
    title: 'Chris Lake<br>& Zane Lowe',
    desc: 'The UK producer talks about his album.',
    image: null,
    logo: 'Music'
  },
  {
    type: 'release',
    title: 'Red Hood - Single',
    subtitle: 'Zach Hood',
    desc: '2025',
    image: null,
    logo: 'Music'
  },
  {
    type: 'station',
    title: 'Danny Rico’s Station',
    bgClass: 'bg-gradient-pink',
    image: null,
    logo: 'Music'
  }
]

const recentlyPlayed = [
  { title: 'Jupiter', artist: 'Nao', cover: 'https://placehold.co/200x200/2980b9/fff?text=Jupiter' },
  { title: 'Mix Tape', artist: 'Mix', cover: 'https://placehold.co/200x200/e74c3c/fff?text=Mix+Tape' },
  { title: 'Coco Jones', artist: 'Artist', cover: 'https://placehold.co/200x200/8e44ad/fff?text=Coco' },
  { title: 'Yuno', artist: 'Artist', cover: 'https://placehold.co/200x200/16a085/fff?text=Yuno' },
  { title: 'Dance', artist: 'Radio', cover: 'https://placehold.co/200x200/f39c12/fff?text=Dance' },
  { title: 'Vibes', artist: 'Playlist', cover: 'https://placehold.co/200x200/2c3e50/fff?text=Vibes' }
]

const currentTrack = ref({
  title: 'All Of Me',
  artist: 'Nao — Jupiter',
  cover: 'https://placehold.co/100x100/444444/fff?text=Nao'
})
</script>

<template>
  <div class="app-background">
    <!-- 去掉了彩色 Blob，只保留干净的灰色背景 -->

    <div class="app-layout">

      <!-- === 左侧侧边栏 (保持悬浮玻璃卡片) === -->
      <aside class="sidebar-floating glass-panel">

        <!-- 导航区 -->
        <nav class="nav-section first-nav">
          <div
            v-for="item in navItems"
            :key="item.id"
            class="nav-item"
            :class="{ active: item.active }"
          >
            <!-- SVG Icons -->
            <svg v-if="item.icon === 'search'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" /></svg>
            <svg v-if="item.icon === 'home'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" /></svg>
            <svg v-if="item.icon === 'grid'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" /></svg>
            <svg v-if="item.icon === 'radio'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>

            {{ item.name }}
          </div>
        </nav>

        <div class="nav-group-title">Library</div>
        <nav class="nav-section">
          <div v-for="item in libraryItems" :key="item.id" class="nav-item">
            <div class="icon-generic"></div>
            {{ item.name }}
          </div>
        </nav>

        <div class="user-profile">
          <img src="https://placehold.co/40x40/222/fff?text=K" alt="User" class="avatar">
          <span class="username">Danny Rico</span>
        </div>
      </aside>

      <!-- === 右侧内容区 (已修改：移除卡片背景，与灰色底色融合) === -->
      <div class="content-area-blended">

        <main class="scrollable-content">
          <h1 class="page-title">Home</h1>

          <!-- Top Picks -->
          <section class="section">
            <div class="section-header">
              <h2>Top Picks for You</h2>
            </div>
            <div class="cards-grid">
              <div
                v-for="(card, index) in topPicks"
                :key="index"
                class="large-card"
                :class="card.bgClass"
                :style="card.image ? { backgroundImage: `url(${card.image})` } : {}"
              >
                <div class="card-overlay"></div>
                <div class="card-content">
                  <div class="card-top">
                    <span v-if="card.logo" class="logo-text">Music</span>
                  </div>
                  <div class="card-center">
                    <h3 v-if="card.title" v-html="card.title"></h3>
                    <h4 v-if="card.subtitle">{{ card.subtitle }}</h4>
                  </div>
                  <div class="card-bottom">
                    <p v-if="card.desc">{{ card.desc }}</p>
                    <div v-if="card.type === 'station'" class="station-tag">{{ card.title }}</div>
                  </div>
                  <button v-if="index === 0" class="play-btn-circle">
                    <svg viewBox="0 0 24 24" fill="currentColor" width="20" height="20"><path d="M8 5v14l11-7z" /></svg>
                  </button>
                </div>
              </div>
            </div>
          </section>

          <!-- Recently Played -->
          <section class="section">
            <div class="section-header-simple">
              <h2>Recently Played <span class="chevron">›</span></h2>
            </div>
            <div class="albums-row">
              <div v-for="(album, index) in recentlyPlayed" :key="index" class="album-item">
                <div class="album-cover">
                  <img :src="album.cover" :alt="album.title">
                  <div class="album-overlay"></div>
                </div>
                <div class="album-info">
                  <div class="album-title">{{ album.title }}</div>
                  <div class="album-artist">{{ album.artist }}</div>
                </div>
              </div>
            </div>
          </section>

          <div class="spacer-bottom"></div>
        </main>

        <!-- === 底部悬浮播放条 (应用 Crystal Dock 质感) === -->
        <div class="player-container-position">
          <div class="player-bar crystal-texture">

            <!-- 播放条内容 -->
            <div class="player-content">
              <!-- Controls -->
              <div class="controls-left">
                <button class="ctrl-btn shuffle">
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M16 3h5v5M4 20L21 3M21 16v5h-5M15 15l-5 5M4 4l5 5"/></svg>
                </button>
                <button class="ctrl-btn prev">
                  <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/></svg>
                </button>
                <!-- Play Button with subtle glow -->
                <button class="ctrl-btn play-pause">
                  <div class="play-btn-glow"></div>
                  <svg width="32" height="32" viewBox="0 0 24 24" fill="currentColor" style="position:relative; z-index:2;"><path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/></svg>
                </button>
                <button class="ctrl-btn next">
                  <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/></svg>
                </button>
                <button class="ctrl-btn loop">
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M17 1l4 4-4 4" /><path d="M3 11V9a4 4 0 0 1 4-4h14" /><path d="M7 23l-4-4 4-4" /><path d="M21 13v2a4 4 0 0 1-4 4H3" /></svg>
                </button>
              </div>

              <!-- Track Info -->
              <div class="track-info">
                <img :src="currentTrack.cover" class="track-cover">
                <div class="track-text">
                  <div class="track-title">{{ currentTrack.title }}</div>
                  <div class="track-artist">{{ currentTrack.artist }}</div>
                </div>
              </div>

              <!-- Volume & More -->
              <div class="controls-right">
                <button class="ctrl-btn">
                  <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>
                </button>
                <div class="volume-control">
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
                  <input type="range" min="0" max="100" value="70">
                </div>
              </div>
            </div>
          </div>
        </div>

      </div>
    </div>
  </div>
</template>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&display=swap');

* { box-sizing: border-box; }

/* === 全局背景：修改为灰色，移除流体 Blob === */
.app-background {
  width: 100vw;
  height: 100vh;
  /* 纯净的现代 UI 灰 */
  background-color: #eff1f5;
  position: relative;
  overflow: hidden;
  font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  color: #1a1a1a; /* 加深字体颜色以适应灰色背景 */
}

.app-layout {
  position: relative;
  z-index: 1;
  display: flex;
  width: 100%;
  height: 100%;
}

/* === 左侧侧边栏 (保持悬浮+玻璃效果) === */
.sidebar-floating {
  width: 260px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  padding: 30px 20px;
  margin: 16px;
  border-radius: 24px;

  /* ★★ 修复高度溢出问题 ★★ */
  height: calc(100% - 32px);

  background: rgba(255, 255, 255, 0.65);
  backdrop-filter: blur(40px) saturate(180%);
  -webkit-backdrop-filter: blur(40px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.8);
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.04);
}

.nav-section { display: flex; flex-direction: column; gap: 4px; margin-bottom: 24px; }
.nav-group-title { font-size: 12px; color: rgba(0,0,0,0.5); font-weight: 600; margin-bottom: 8px; padding-left: 12px; }

.nav-item {
  display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 12px;
  color: #333; font-size: 15px; font-weight: 500; cursor: pointer; transition: all 0.2s;
}
.nav-item:hover { background-color: rgba(255, 255, 255, 0.5); }
.nav-item.active { background-color: #fff; color: #fa233b; box-shadow: 0 2px 12px rgba(0,0,0,0.06); }
.nav-icon { width: 20px; height: 20px; opacity: 0.8; }
.icon-generic { width: 18px; height: 18px; border: 1.5px solid rgba(0,0,0,0.4); border-radius: 4px; }

.user-profile { margin-top: auto; display: flex; align-items: center; gap: 12px; padding-top: 20px;}
.avatar { width: 36px; height: 36px; border-radius: 50%; }
.username { font-weight: 600; font-size: 14px; }

/* === 右侧内容区 (已修改：无背景，无阴影，无边框) === */
.content-area-blended {
  flex: 1;
  /* 关键修改：透明背景，使其与 .app-background 融为一体 */
  background: transparent;
  box-shadow: none;
  border: none;
  border-radius: 0;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
}

/* 调整内边距，因为没有卡片边框了，需要留出视觉呼吸感 */
.scrollable-content {
  flex: 1;
  overflow-y: auto;
  padding: 20px 16px 0 10px; /* 上边距和侧边距微调 */
  padding-bottom: 0;
}

/* 自定义滚动条，使其更隐蔽 */
.scrollable-content::-webkit-scrollbar { width: 8px; }
.scrollable-content::-webkit-scrollbar-thumb { background: rgba(0,0,0,0.1); border-radius: 4px; }
.scrollable-content::-webkit-scrollbar-track { background: transparent; }

.page-title { font-size: 34px; font-weight: 800; margin-bottom: 24px; color: #111; }
.section { margin-bottom: 40px; }
.section-header h2,
.section-header-simple  h2 { font-size: 22px; font-weight: 700; color: #111; padding-bottom: 10px}

/* Grid & Cards */
.cards-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 24px; }

.large-card {
  aspect-ratio: 1 / 1.25; border-radius: 18px; overflow: hidden; position: relative;
  background-color: #e0e0e6; /* 卡片加载前的占位色稍深一点 */
  background-size: cover; background-position: center;
  /* 卡片本身的阴影保留，以从灰色背景中凸显出来 */
  box-shadow: 0 4px 20px rgba(0,0,0,0.06);
  transition: transform 0.25s; cursor: pointer;
}
.large-card:hover { transform: translateY(-4px); }
.card-overlay { position: absolute; inset: 0; background: linear-gradient(to bottom, rgba(0,0,0,0) 30%, rgba(0,0,0,0.7)); }
.bg-gradient-orange { background: linear-gradient(135deg, #ff9a9e 0%, #fecfef 100%); }
.bg-gradient-pink { background: linear-gradient(45deg, #85FFBD 0%, #FFFB7D 100%); }
.card-content { position: absolute; inset: 0; padding: 24px; display: flex; flex-direction: column; justify-content: space-between; color: white; z-index: 2; }
.logo-text { font-size: 11px; font-weight: 700; opacity: 0.9; }
.card-center h3 { font-size: 28px; line-height: 1.05; font-weight: 800; margin: 0; }
.card-center h4 { font-size: 28px; font-weight: 300; margin: 0; opacity: 0.95; }
.card-bottom p { font-size: 13px; font-weight: 500; opacity: 0.9; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; margin: 0; }
.play-btn-circle { position: absolute; bottom: 20px; right: 20px; width: 40px; height: 40px; border-radius: 50%; background: rgba(255,255,255,0.25); backdrop-filter: blur(10px); border: 1px solid rgba(255,255,255,0.4); color: white; display: flex; align-items: center; justify-content: center; cursor: pointer; }

/* Recently Played */
.albums-row { display: flex; gap: 24px; overflow-x: auto; padding-bottom: 20px; margin: 0 -10px; padding-left: 10px; }
.album-item { width: 160px; flex-shrink: 0; cursor: pointer; transition: opacity 0.2s; }
.album-cover { width: 100%; aspect-ratio: 1; border-radius: 10px; overflow: hidden; margin-bottom: 10px; box-shadow: 0 4px 12px rgba(0,0,0,0.06); }
.album-cover img { width: 100%; height: 100%; object-fit: cover; }
.album-title { font-size: 14px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: #111; }
.album-artist { font-size: 13px; color: #666; }
.spacer-bottom { height: 120px; }

/* === 底部悬浮播放条 (应用 Crystal Texture) === */
.player-container-position {
  position: absolute;
  bottom: 20px;
  left: 0;
  width: 100%;
  display: flex;
  justify-content: center;
  z-index: 50;
  pointer-events: none;
}

.crystal-texture {
  pointer-events: auto;
  width: calc(100% - 40px); /* 宽度略微增加 */
  max-width: 850px;
  height: 80px;
  padding: 0 24px;

  /* 针对灰色背景调整：增加不透明度，使其更突出 */
  background: rgba(255, 255, 255, 0.7);
  backdrop-filter: blur(25px) saturate(180%);
  -webkit-backdrop-filter: blur(25px) saturate(180%);

  border-radius: 28px;
  border: 1px solid rgba(255, 255, 255, 0.8);
  box-shadow:
    0 20px 50px rgba(0, 0, 0, 0.1),
    0 1px 0px rgba(255, 255, 255, 0.9) inset;

  isolation: isolate;
}

.player-content {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 100%;
}

.controls-left, .controls-right { display: flex; align-items: center; gap: 20px; flex: 1; }
.controls-right { justify-content: flex-end; }

.ctrl-btn {
  background: none; border: none; padding: 0;
  color: #333; cursor: pointer; transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
  display: flex; align-items: center; justify-content: center; position: relative;
}
.ctrl-btn:hover { transform: scale(1.15); color: #000; }

.play-btn-glow {
  position: absolute; width: 100%; height: 100%;
  background: rgba(255, 255, 255, 0.6);
  filter: blur(10px); border-radius: 50%; z-index: 1;
  opacity: 0; transition: opacity 0.3s;
}
.ctrl-btn.play-pause:hover .play-btn-glow { opacity: 1; }

.track-info {
  flex: 0 0 auto;
  display: flex; align-items: center; gap: 14px;
  /* 轨道信息卡片背景微调 */
  background: rgba(255,255,255,0.4);
  padding: 6px 16px 6px 6px;
  border-radius: 12px;
  box-shadow: 0 2px 10px rgba(0,0,0,0.02);
  margin: 0 20px;
  border: 1px solid rgba(255,255,255,0.3);
}
.track-cover { width: 44px; height: 44px; border-radius: 8px; display: block; box-shadow: 0 2px 5px rgba(0,0,0,0.1); }
.track-text { display: flex; flex-direction: column; }
.track-title { font-size: 13px; font-weight: 700; color: #111; }
.track-artist { font-size: 12px; color: #555; }

.volume-control { display: flex; align-items: center; gap: 8px; color: #555; }
input[type=range] { width: 80px; accent-color: #333; cursor: pointer; }

@media (max-width: 1000px) {
  .track-info { display: none; }
  .sidebar-floating { width: 80px; padding: 20px 10px; align-items: center; }
  .nav-item { justify-content: center; padding: 12px; }
  .nav-item svg { width: 24px; height: 24px; }
  .nav-section, .nav-group-title, .username, .user-profile img { display: none; }
}
</style>
