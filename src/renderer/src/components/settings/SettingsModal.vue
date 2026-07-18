<script setup lang="ts">
import AppIcon from '../AppIcon.vue'
import noiseSvg from '@renderer/assets/icons/noise.svg?url'
import { computed, ref, type Component } from 'vue'

const noiseBg = `url(${noiseSvg})`
import '@renderer/assets/settings.css'
import GeneralSettingsTab from './GeneralSettingsTab.vue'
import AudioSettingsTab from './AudioSettingsTab.vue'
import AppearanceSettingsTab from './AppearanceSettingsTab.vue'
import LibrarySettingsTab from './LibrarySettingsTab.vue'
import ShortcutsSettingsTab from './ShortcutsSettingsTab.vue'
import AboutSettingsTab from './AboutSettingsTab.vue'
import SettingsSidebar from './SettingsSidebar.vue'
import CacheSettingsTab from './CacheSettingsTab.vue'
import DebugSettingsTab from './DebugSettingsTab.vue'

type SettingsTabId =
  | 'general'
  | 'audio'
  | 'appearance'
  | 'library'
  | 'shortcuts'
  | 'about'
  | 'cache'
  | 'debug'

type SettingsTab = {
  id: SettingsTabId
  name: string
  icon: string
}

const tabs: SettingsTab[] = [
  {
    id: 'general',
    name: '通用',
    icon: 'settings-general'
  },
  {
    id: 'audio',
    name: '音频输出',
    icon: 'settings-audio'
  },
  {
    id: 'appearance',
    name: '外观个性化',
    icon: 'settings-appearance'
  },
  {
    id: 'library',
    name: '曲库管理',
    icon: 'settings-library'
  },
  {
    id: 'cache',
    name: '缓存',
    icon: 'settings-cache'
  },
  {
    id: 'shortcuts',
    name: '快捷键',
    icon: 'settings-shortcuts'
  },
  {
    id: 'debug',
    name: '调试',
    icon: 'settings-debug'
  },
  {
    id: 'about',
    name: '关于软件',
    icon: 'settings-about'
  }
]

const tabComponents: Record<SettingsTabId, Component> = {
  general: GeneralSettingsTab,
  audio: AudioSettingsTab,
  appearance: AppearanceSettingsTab,
  library: LibrarySettingsTab,
  shortcuts: ShortcutsSettingsTab,
  about: AboutSettingsTab,
  cache: CacheSettingsTab,
  debug: DebugSettingsTab
}

const activeTab = ref<SettingsTabId>('general')

const activeTabTitle = computed(() => {
  return tabs.find((tab) => tab.id === activeTab.value)?.name ?? ''
})

const activeTabComponent = computed(() => tabComponents[activeTab.value])

const closeSettingsWindow = (): void => {
  window.api.close_settings_window()
}
</script>

<template>
  <div class="settings-wrapper">
    <div class="settings-window liquid-glass">
      <SettingsSidebar v-model:active-tab="activeTab" :tabs="tabs" class="sidebar-area" />

      <main class="settings-main">
        <header class="main-header">
          <h2>{{ activeTabTitle }}</h2>
          <button class="close-btn" aria-label="关闭设置" @click="closeSettingsWindow">
            <AppIcon name="close" :size="18" />
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
  color: var(--sys-text);
  font-family:
    -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Helvetica Neue', Arial, sans-serif;
}

/* 外部包装器，假设用于居中 Modal，或者铺满全屏 */
.settings-wrapper {
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.28);
  z-index: 9999;
  border-radius: 8px;
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
    v-bind(noiseBg),
    linear-gradient(135deg, var(--sys-surface-strong) 0%, var(--sys-surface) 100%);

  backdrop-filter: blur(60px) saturate(200%);
  -webkit-backdrop-filter: blur(60px) saturate(200%);
  border: 1px solid var(--sys-border);

  /* 优化3：强制硬件加速，防止在某些浏览器/Electron中毛玻璃退化变全透 */
  transform: translateZ(0);
}

/* 侧边栏区域隔离线 */
.sidebar-area {
  border-right: 1px solid var(--sys-border);
  /* 稍微提高一点白底，与整体匹配 */
  background: var(--sys-surface-muted);
}

/* 右侧主内容区 */
.settings-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  position: relative;
  /* 优化4：右侧内容区提高白底比例(0.6)，保证文字、表单清晰度不受底层背景干扰 */
  background: var(--sys-surface-muted);
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
  letter-spacing: 0;
  color: var(--sys-text);
  margin: 0;
  text-shadow: 0 1px 2px rgba(255, 255, 255, 0.8); /* 文字雕刻感 */
}

/* 苹果风圆形微质感关闭按钮 */
.close-btn {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: var(--sys-control);
  border: 1px solid var(--sys-border);
  color: var(--sys-text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.4);
}

.close-btn:hover {
  background: var(--sys-control-hover);
  color: var(--sys-text);
  transform: scale(1.05);
  box-shadow:
    0 2px 8px rgba(0, 0, 0, 0.05),
    inset 0 1px 0 rgba(255, 255, 255, 0.5);
}

.close-btn:active {
  transform: scale(0.95);
  background: var(--sys-control-active);
}

/* 内容滚动区 */
.content-scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 16px 40px 48px;
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
