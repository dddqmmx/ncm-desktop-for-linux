<script setup lang="ts">
import { ref } from 'vue'
import { SongDetail } from 'NeteaseCloudMusicApi'

// --- 模拟数据 ---

const navItems = [
  { id: 1, name: 'Home', active: true, icon: 'home' },
  { id: 2, name: 'Discover', icon: 'grid' }, // 改名为 Discover
  { id: 3, name: 'Radio', icon: 'radio' }
]

const libraryItems = [
  { id: 4, name: 'Favorites', icon: 'heart' }, // 改名
  { id: 5, name: 'Local Files', icon: 'folder' },
  { id: 6, name: 'Artists', icon: 'mic' },
  { id: 7, name: 'Albums', icon: 'album' },
]

const topPicks = [
  {
    type: 'mix',
    title: 'Daily Mix 1',
    subtitle: 'Chill Vibes',
    image: 'https://placehold.co/400x500/A7C5EB/fff?text=Chill',
  },
  {
    type: 'show',
    title: 'Design Talks',
    subtitle: 'Podcast',
    image: 'https://placehold.co/400x500/E3E3E3/333?text=Talks',
  },
  {
    type: 'release',
    title: 'Midnight',
    subtitle: 'The Weeknd',
    image: 'https://placehold.co/400x500/222/fff?text=Starboy',
  },
  {
    type: 'station',
    title: 'Deep Focus',
    subtitle: 'Instrumental',
    image: 'https://placehold.co/400x500/C4D7D1/fff?text=Focus',
  }
]

const recentlyPlayed = [
  { title: 'Neon Nights', artist: 'Synthwave', cover: 'https://placehold.co/200x200/333/fff?text=Neon' },
  { title: 'Pure Water', artist: 'Skepta', cover: 'https://placehold.co/200x200/81d4fa/fff?text=Water' },
  { title: 'Velvet', artist: 'JMSN', cover: 'https://placehold.co/200x200/e1bee7/fff?text=Velvet' },
  { title: 'Abstract', artist: 'Shapes', cover: 'https://placehold.co/200x200/b0bec5/fff?text=Art' },
  { title: 'Focus', artist: 'H.E.R.', cover: 'https://placehold.co/200x200/ffcc80/fff?text=Focus' },
]

const currentTrack = ref({
  title: 'Glacier',
  artist: 'James Blake',
  cover: 'https://placehold.co/100x100/A7C5EB/fff?text=JB'
})
</script>

<template>
  <div class="app-background">
    <div class="app-layout">

      <!-- === 左侧侧边栏 (极简风格，无头像，无搜索) === -->
      <aside class="sidebar-minimal glass-panel">

        <nav class="nav-section">
          <div
            v-for="item in navItems"
            :key="item.id"
            class="nav-item"
            :class="{ active: item.active }"
          >
            <span class="nav-icon-wrapper">
              <svg v-if="item.icon === 'home'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" /></svg>
              <svg v-if="item.icon === 'grid'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" /></svg>
              <svg v-if="item.icon === 'radio'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>
            </span>
            <span class="nav-text">{{ item.name }}</span>
          </div>
        </nav>

        <div class="divider"></div>

        <nav class="nav-section secondary">
          <div class="nav-label">Library</div>
          <div v-for="item in libraryItems" :key="item.id" class="nav-item secondary-item">
            {{ item.name }}
          </div>
        </nav>
      </aside>

      <!-- === 右侧内容区 === -->
      <div class="content-area">

        <!-- 顶部 Header (搜索 + 用户) - 打破 Apple Music 布局 -->
        <header class="top-bar">
          <div class="search-glass">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="search-icon"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
            <input type="text" placeholder="Search artists, songs..." />
          </div>

          <div class="user-actions">
            <button class="icon-btn">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 21a2 2 0 0 1-3.46 0"/></svg>
            </button>
            <div class="avatar-glass">
              <img src="https://placehold.co/40x40/333/fff?text=D" alt="User">
            </div>
          </div>
        </header>

        <main class="scrollable-content">
          <!-- Hero Cards Grid -->
          <section class="section">
            <div class="section-header">
              <h2>Featured</h2>
            </div>
            <div class="cards-grid">
              <div
                v-for="(card, index) in topPicks"
                :key="index"
                class="modern-card"
                :style="`background-image: url(${card.image})`"
              >
                <!-- 内部悬浮玻璃信息条 -->
                <div class="inner-glass-info">
                  <div class="info-text">
                    <h3>{{ card.title }}</h3>
                    <p>{{ card.subtitle }}</p>
                  </div>
                  <button class="play-mini-btn">
                    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16"><path d="M8 5v14l11-7z" /></svg>
                  </button>
                </div>
              </div>
            </div>
          </section>

          <!-- Recently Played -->
          <section class="section">
            <div class="section-header">
              <h2>Recent</h2>
            </div>
            <div class="list-row">
              <div v-for="(album, index) in recentlyPlayed" :key="index" class="list-item">
                <div class="list-cover-wrapper">
                  <img :src="album.cover" :alt="album.title">
                  <div class="hover-play">
                    <svg viewBox="0 0 24 24" fill="currentColor" width="24" height="24"><path d="M8 5v14l11-7z" /></svg>
                  </div>
                </div>
                <div class="list-info">
                  <div class="list-title">{{ album.title }}</div>
                  <div class="list-artist">{{ album.artist }}</div>
                </div>
              </div>
            </div>
          </section>

          <div class="spacer-bottom"></div>
        </main>

        <!-- === 底部悬浮播放胶囊 (更窄、更厚重的水晶感) === -->
        <div class="player-dock-container">
          <div class="player-capsule">

            <div class="capsule-left">
              <div class="spinning-disc">
                 <img :src="currentTrack.cover" class="disc-img">
              </div>
              <div class="capsule-info">
                <span class="c-title">{{ currentTrack.title }}</span>
                <span class="c-artist">{{ currentTrack.artist }}</span>
              </div>
            </div>

            <div class="capsule-center">
              <button class="c-btn prev">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/></svg>
              </button>
              <button class="c-btn play-main">
                <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z" /></svg>
              </button>
              <button class="c-btn next">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/></svg>
              </button>
            </div>

            <div class="capsule-right">
              <div class="volume-bar-mini">
                <div class="vol-level"></div>
              </div>
            </div>

          </div>
        </div>

      </div>
    </div>
  </div>
</template>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=Manrope:wght@400;500;600;700;800&display=swap');

* { box-sizing: border-box; }

.app-background {
  width: 100vw;
  height: 100vh;
  /* 冷灰色背景，更现代 */
  background-color: #F2F4F7;
  color: #1a1a1a;
  font-family: 'Manrope', sans-serif; /* 使用 Manrope 字体，比 Inter 更具几何感 */
  overflow: hidden;
}

.app-layout {
  display: flex;
  width: 100%;
  height: 100%;
}

/* === 左侧侧边栏 === */
.sidebar-minimal {
  width: 240px;
  flex-shrink: 0;
  height: calc(100% - 40px);
  margin: 20px 0 20px 20px;
  padding: 30px 20px;
  border-radius: 24px;

  /* 极浅的玻璃，几乎透明 */
  background: rgba(255, 255, 255, 0.4);
  backdrop-filter: blur(20px);
  border: 1px solid rgba(255,255,255,0.6);
  display: flex;
  flex-direction: column;
}

.nav-section { display: flex; flex-direction: column; gap: 8px; }

/* 导航项设计：选中时为深色胶囊 */
.nav-item {
  display: flex; align-items: center; gap: 14px;
  padding: 12px 16px;
  border-radius: 16px; /* 更圆 */
  color: #666;
  font-weight: 600;
  font-size: 14px;
  cursor: pointer;
  transition: all 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
}

.nav-icon { width: 20px; height: 20px; }

/* 选中状态：黑色背景，白色文字，投影 */
.nav-item.active {
  background-color: #1a1a1a;
  color: #fff;
  box-shadow: 0 8px 20px rgba(0,0,0,0.15);
}

.divider { height: 1px; background: rgba(0,0,0,0.05); margin: 24px 10px; }

.nav-section.secondary .nav-label {
  font-size: 11px; text-transform: uppercase; letter-spacing: 1px;
  color: #999; margin-bottom: 10px; padding-left: 16px;
}
.secondary-item {
  font-weight: 500; font-size: 14px; color: #555; padding-left: 16px;
}
.secondary-item:hover { color: #000; background: rgba(255,255,255,0.4); }

/* === 右侧内容区 === */
.content-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
}

/* 顶部 Header */
.top-bar {
  display: flex; justify-content: space-between; align-items: center;
  padding: 20px 40px;
  height: 80px;
  z-index: 10;
}

.search-glass {
  display: flex; align-items: center; gap: 10px;
  background: rgba(255,255,255,0.5);
  border: 1px solid rgba(255,255,255,0.8);
  padding: 10px 16px;
  border-radius: 50px;
  width: 300px;
  transition: width 0.3s;
}
.search-glass:focus-within { width: 340px; background: #fff; box-shadow: 0 4px 20px rgba(0,0,0,0.05); }
.search-icon { color: #888; }
.search-glass input {
  border: none; background: transparent; outline: none;
  font-family: inherit; font-size: 14px; width: 100%; color: #333;
}

.user-actions { display: flex; align-items: center; gap: 20px; }
.icon-btn { border: none; background: none; cursor: pointer; color: #555; padding: 8px; border-radius: 50%; transition: background 0.2s; }
.icon-btn:hover { background: rgba(0,0,0,0.05); color: #000; }
.avatar-glass img { width: 40px; height: 40px; border-radius: 12px; border: 2px solid #fff; box-shadow: 0 4px 10px rgba(0,0,0,0.1); }

/* 内容滚动区 */
.scrollable-content {
  flex: 1; overflow-y: auto; padding: 0 40px;
  scrollbar-width: none;
}
.scrollable-content::-webkit-scrollbar { display: none; }

.section-header { margin-bottom: 20px; display: flex; align-items: flex-end; }
.section-header h2 { font-size: 24px; font-weight: 800; color: #1a1a1a; letter-spacing: -0.5px; margin: 0; }

/* === 现代卡片设计 (Featured) === */
.cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 24px;
}

.modern-card {
  aspect-ratio: 3 / 4;
  border-radius: 24px;
  background-size: cover;
  background-position: center;
  position: relative;
  overflow: hidden;
  box-shadow: 0 10px 30px rgba(0,0,0,0.05);
  transition: transform 0.3s ease;
  cursor: pointer;
}
.modern-card:hover { transform: translateY(-5px); }

/* 画中画玻璃条 */
.inner-glass-info {
  position: absolute;
  bottom: 16px; left: 16px; right: 16px;
  height: 70px;
  background: rgba(255, 255, 255, 0.75);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border-radius: 16px;
  padding: 12px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  box-shadow: 0 4px 15px rgba(0,0,0,0.08);
}
.info-text h3 { margin: 0; font-size: 15px; font-weight: 700; color: #111; }
.info-text p { margin: 2px 0 0 0; font-size: 12px; color: #555; }
.play-mini-btn {
  width: 32px; height: 32px; border-radius: 50%; border: none;
  background: #111; color: #fff;
  display: flex; align-items: center; justify-content: center;
  cursor: pointer; transition: transform 0.2s;
}
.play-mini-btn:hover { transform: scale(1.1); }


/* === 列表设计 (Recent) === */
.list-row { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 30px; }

.list-item { cursor: pointer; group: true; }
.list-cover-wrapper {
  position: relative; border-radius: 16px; overflow: hidden; aspect-ratio: 1;
  margin-bottom: 12px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.06);
}
.list-cover-wrapper img { width: 100%; height: 100%; object-fit: cover; transition: transform 0.5s; }
.list-item:hover img { transform: scale(1.05); }

.hover-play {
  position: absolute; inset: 0; background: rgba(0,0,0,0.2);
  display: flex; align-items: center; justify-content: center;
  opacity: 0; transition: opacity 0.3s; color: #fff;
}
.list-item:hover .hover-play { opacity: 1; }

.list-title { font-weight: 700; font-size: 14px; margin-bottom: 4px; color: #222; }
.list-artist { font-size: 12px; color: #888; }

.spacer-bottom { height: 120px; }

/* === 播放胶囊 (Crystal Capsule) === */
.player-dock-container {
  position: absolute;
  bottom: 30px; left: 0; width: 100%;
  display: flex; justify-content: center;
  pointer-events: none; /* 让鼠标能穿过容器空白处 */
}

.player-capsule {
  pointer-events: auto;
  width: 600px;
  height: 72px;
  background: rgba(255, 255, 255, 0.65);
  backdrop-filter: blur(30px) saturate(150%);
  -webkit-backdrop-filter: blur(30px) saturate(150%);
  border-radius: 100px; /* 完全药丸形状 */
  border: 1px solid rgba(255, 255, 255, 0.8);
  box-shadow:
    0 20px 60px rgba(0,0,0,0.1),
    inset 0 1px 0 rgba(255,255,255,0.9);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
}

.capsule-left {
  display: flex; align-items: center; gap: 12px;
  background: #fff; /* 左侧信息单独一个白底块，增加层次 */
  padding: 6px 16px 6px 6px;
  border-radius: 50px;
  box-shadow: 0 2px 10px rgba(0,0,0,0.03);
}

.spinning-disc {
  width: 40px; height: 40px; border-radius: 50%; overflow: hidden;
  animation: spin 10s linear infinite;
}
.disc-img { width: 100%; height: 100%; object-fit: cover; }

.capsule-info { display: flex; flex-direction: column; padding-right: 10px; }
.c-title { font-size: 13px; font-weight: 700; color: #111; }
.c-artist { font-size: 11px; color: #666; }

.capsule-center {
  display: flex; align-items: center; gap: 16px;
}
.c-btn {
  background: none; border: none; padding: 0; cursor: pointer; color: #444;
  transition: transform 0.2s, color 0.2s;
  display: flex; align-items: center; justify-content: center;
}
.c-btn:hover { color: #000; transform: scale(1.1); }
.c-btn.play-main {
  width: 44px; height: 44px; background: #1a1a1a; color: #fff;
  border-radius: 50%; box-shadow: 0 4px 12px rgba(0,0,0,0.2);
}
.c-btn.play-main:hover { transform: scale(1.05); background: #000; }

.capsule-right {
  padding-right: 20px; width: 100px; display: flex; justify-content: flex-end;
}
.volume-bar-mini {
  width: 80px; height: 4px; background: rgba(0,0,0,0.1); border-radius: 2px;
  position: relative; overflow: hidden;
}
.vol-level {
  position: absolute; left: 0; top: 0; bottom: 0; width: 60%;
  background: #1a1a1a;
}

@keyframes spin { 100% { transform: rotate(360deg); } }

@media (max-width: 900px) {
  .sidebar-minimal { display: none; }
  .scrollable-content { padding: 0 20px; }
  .player-capsule { width: 90%; }
}
</style>
