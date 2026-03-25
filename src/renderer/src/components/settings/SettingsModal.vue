<script setup lang="ts">
import { computed, ref, type Component } from 'vue'
import '@renderer/assets/settings.css'
import GeneralSettingsTab from './GeneralSettingsTab.vue'
import AudioSettingsTab from './AudioSettingsTab.vue'
import AppearanceSettingsTab from './AppearanceSettingsTab.vue'
import LibrarySettingsTab from './LibrarySettingsTab.vue'
import ShortcutsSettingsTab from './ShortcutsSettingsTab.vue'
import AboutSettingsTab from './AboutSettingsTab.vue'
import SettingsSidebar from './SettingsSidebar.vue'
import CacheSettingsTab from './CacheSettingsTab.vue'

type SettingsTabId = 'general' | 'audio' | 'appearance' | 'library' | 'shortcuts' | 'about' | 'cache'

type SettingsTab = {
  id: SettingsTabId
  name: string
  icon: string
}

const emit = defineEmits<{ close: [] }>()

const tabs: SettingsTab[] = [
  { id: 'general', name: '通用', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4' },
  { id: 'audio', name: '音频输出', icon: 'M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z' },
  { id: 'appearance', name: '外观个性化', icon: 'M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-3' },
  { id: 'library', name: '曲库管理', icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z' },
  { id: 'cache', name: '缓存', icon: 'M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4' },
  { id: 'shortcuts', name: '快捷键', icon: 'M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25' },
  { id: 'about', name: '关于软件', icon: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z' }
]

const tabComponents: Record<SettingsTabId, Component> = {
  general: GeneralSettingsTab,
  audio: AudioSettingsTab,
  appearance: AppearanceSettingsTab,
  library: LibrarySettingsTab,
  shortcuts: ShortcutsSettingsTab,
  about: AboutSettingsTab,
  cache: CacheSettingsTab
}

const activeTab = ref<SettingsTabId>('general')

const activeTabTitle = computed(() => {
  return tabs.find((tab) => tab.id === activeTab.value)?.name ?? ''
})

const activeTabComponent = computed(() => tabComponents[activeTab.value])
</script>

<template>
  <div class="settings-wrapper">
    <div class="settings-window liquid-glass">
      <SettingsSidebar v-model:active-tab="activeTab" :tabs="tabs" class="sidebar-area" />

      <main class="settings-main">
        <header class="main-header">
          <h2>{{ activeTabTitle }}</h2>
          <button class="close-btn" @click="emit('close')" aria-label="关闭设置">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </header>

        <div class="content-scroll">
          <transition name="fade-slide" mode="out-in">
            <keep-alive>
              <component :is="activeTabComponent" :key="activeTab" />
            </keep-alive>
          </transition>
        </div>
      </main>
    </div>
  </div>
</template>

<style scoped>
/* 基础重置与字体引入 */
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
  color: #000;
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Arial, sans-serif;
}

/* 外部包装器，假设用于居中 Modal，或者铺满全屏 */
.settings-wrapper {
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.3);
  z-index: 9999;
  border-radius: 20px;
  animation: appleScaleUp 0.5s cubic-bezier(0.2, 0.8, 0.2, 1);
}

/* 核心：液态玻璃窗口 */
.settings-window {
  width: 100%;
  height: 100%;
  border-radius: 20px;
  display: flex;
  overflow: hidden;
  -webkit-app-region: no-drag;
}

/* 苹果液态玻璃材质 */
.liquid-glass {
  /* 优化2：提高基础白色的不透明度(0.85/0.75)；加入极其微弱的噪点纹理增加物理质感 */
  background:
    url('data:image/svg+xml;utf8,%3Csvg viewBox="0 0 200 200" xmlns="http://www.w3.org/2000/svg"%3E%3Cfilter id="noiseFilter"%3E%3CfeTurbulence type="fractalNoise" baseFrequency="0.65" numOctaves="3" stitchTiles="stitch"/%3E%3C/filter%3E%3Crect width="100%25" height="100%25" filter="url(%23noiseFilter)" opacity="0.04"/%3E%3C/svg%3E'),
    linear-gradient(
      135deg,
      rgba(250, 250, 252, 0.85) 0%,
      rgba(242, 242, 247, 0.75) 100%
    );

  backdrop-filter: blur(60px) saturate(200%);
  -webkit-backdrop-filter: blur(60px) saturate(200%);
  border: 1px solid rgba(255, 255, 255, 0.5);

  /* 优化3：强制硬件加速，防止在某些浏览器/Electron中毛玻璃退化变全透 */
  transform: translateZ(0);
}

/* 侧边栏区域隔离线 */
.sidebar-area {
  border-right: 1px solid rgba(0, 0, 0, 0.06);
  /* 稍微提高一点白底，与整体匹配 */
  background: rgba(255, 255, 255, 0.2);
}

/* 右侧主内容区 */
.settings-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  position: relative;
  /* 优化4：右侧内容区提高白底比例(0.6)，保证文字、表单清晰度不受底层背景干扰 */
  background: rgba(255, 255, 255, 0.6);
  min-width: 0;
}

/* 吸顶 Header，带有向下的渐隐毛玻璃效果 */
.main-header {
  padding: 28px 40px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  position: sticky;
  top: 0;
  z-index: 10;
}

.main-header h2 {
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.5px;
  color: #1d1d1f; /* 苹果标志性的深灰而非死黑 */
  margin: 0;
  text-shadow: 0 1px 2px rgba(255, 255, 255, 0.8); /* 文字雕刻感 */
}

/* 苹果风圆形微质感关闭按钮 */
.close-btn {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: rgba(0, 0, 0, 0.04);
  border: 1px solid rgba(0, 0, 0, 0.02);
  color: #555;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.4);
}

.close-btn:hover {
  background: rgba(0, 0, 0, 0.08);
  color: #000;
  transform: scale(1.05);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.05), inset 0 1px 0 rgba(255, 255, 255, 0.5);
}

.close-btn:active {
  transform: scale(0.95);
  background: rgba(0, 0, 0, 0.12);
}

/* 内容滚动区 */
.content-scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 16px 40px 48px;
}

/* 流体隐藏式滚动条 (macOS style) */
.content-scroll::-webkit-scrollbar {
  width: 14px; /* 留出可点击范围 */
  background: transparent;
}

.content-scroll::-webkit-scrollbar-track {
  background: transparent;
}

.content-scroll::-webkit-scrollbar-thumb {
  background-clip: padding-box;
  background-color: rgba(0, 0, 0, 0.15);
  border: 4px solid transparent; /* 使用透明边框把滚动条挤细，实现悬浮效果 */
  border-radius: 10px;
}

.content-scroll::-webkit-scrollbar-thumb:hover {
  background-color: rgba(0, 0, 0, 0.3);
}

/* 切换 Tab 时的平滑过渡动画 */
.fade-slide-enter-active,
.fade-slide-leave-active {
  transition: all 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
}

.fade-slide-enter-from {
  opacity: 0;
  transform: translateY(10px);
}

.fade-slide-leave-to {
  opacity: 0;
  transform: translateY(-10px);
}

/* 初始弹出动画 */
@keyframes appleScaleUp {
  0% {
    opacity: 0;
    transform: scale(0.92) translateY(16px);
  }
  100% {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
</style>
