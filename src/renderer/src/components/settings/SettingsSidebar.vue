<template>
  <aside class="settings-sidebar">
    <div class="sidebar-header">
      <div class="app-brand">
        <div class="brand-dot"></div>
        <span>SETTINGS</span>
      </div>
    </div>
    <nav class="sidebar-nav">
      <div class="nav-indicator" :style="indicatorStyle"></div>
      <button
        v-for="tab in tabs"
        :key="tab.id"
        class="nav-item"
        :class="{ active: activeTab === tab.id }"
        @click="$emit('update:activeTab', tab.id)"
      >
        <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" :d="tab.icon" />
        </svg>
        {{ tab.name }}
      </button>
    </nav>
  </aside>
</template>

<script setup lang="ts">
import { computed } from 'vue'

type SettingsTab = {
  id: string
  name: string
  icon: string
}

const props = defineProps<{ tabs: SettingsTab[]; activeTab: string }>()

const indicatorStyle = computed((): { transform: string } => {
  const index = props.tabs.findIndex((t) => t.id === props.activeTab)
  return { transform: `translateY(${index * 52}px)` }
})
</script>

<style scoped>
.settings-sidebar {
  width: 240px;
  background-color: #fbfbfb; /* 侧边栏给一个极浅的灰底，更能衬托出指示器的白色卡片 */
  border-right: 1px solid rgba(0, 0, 0, 0.06);
  display: flex;
  flex-direction: column;
  padding: 32px 16px;
  -webkit-app-region: drag;
}

.app-brand {
  padding: 0 16px 32px;
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 11px;
  font-weight: 800;
  letter-spacing: 1.5px;
  color: rgba(0, 0, 0, 0.4);
}

.brand-dot {
  width: 8px;
  height: 8px;
  background: #111;
  border-radius: 2px;
}

.sidebar-nav {
  position: relative;
  flex: 1;
  overflow-y: auto;
  scroll-behavior: smooth;
}

/* =========================================
   核心优化：滑动指示器 (The Indicator)
   ========================================= */
.nav-indicator {
  height: 52px;
  position: absolute;
  left: 0;
  top: 0;
  width: 100%;
  height: 44px; /* 必须设置高度，与 nav-item 保持一致 */
  background: #ffffff; /* 纯白背景 */
  border-radius: 10px;
  /* 添加细腻的弥散阴影和微小边框，营造悬浮感 */
  border: 1px solid rgba(0, 0, 0, 0.03);
  transition: transform 0.4s cubic-bezier(0.2, 1, 0.3, 1);
  z-index: 0;
}

/* 指示器左侧的强调竖线 (可选，增加现代感) */
.nav-indicator::before {
  content: '';
  position: absolute;
  left: 4px; /* 距离左侧一点距离 */
  top: 50%;
  transform: translateY(-50%);
  width: 4px;
  height: 20px;
  border-radius: 4px;
}

/* =========================================
   导航项 (Nav Items)
   ========================================= */
.nav-item {
  position: relative;
  width: 100%;
  height: 44px;
  margin-bottom: 8px;
  border: none;
  background: none;
  display: flex;
  align-items: center;
  padding: 0 16px;
  gap: 12px;
  font-size: 14px;
  font-weight: 600;
  color: rgba(0, 0, 0, 0.45);
  cursor: pointer;
  z-index: 1; /* 确保文字在指示器上方 */
  -webkit-app-region: no-drag;
  /* 添加所有状态变化的平滑过渡 */
  transition: color 0.2s ease, background-color 0.2s ease;
  border-radius: 10px;
}

/* 未选中状态下的 Hover 效果 */
.nav-item:hover:not(.active) {
  color: rgba(0, 0, 0, 0.7);
  background: rgba(0, 0, 0, 0.03); /* 轻轻的背景色 */
}

/* 选中状态 */
.nav-item.active {
  color: #111;
  /* 选中时不需要 hover 背景，因为底部已经有指示器了 */
  background: transparent;
}

/* 图标基础样式 */
.nav-icon {
  width: 18px;
  height: 18px;
  transition: transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1); /* 添加弹性过渡 */
}

/* 选中时，图标有一个微弱的放大弹出效果 */
.nav-item.active .nav-icon {
  transform: scale(1.15);
  stroke-width: 2.2; /* 可选：让图标变粗一点点 */
}

/* =========================================
   滚动条美化
   ========================================= */
.sidebar-nav::-webkit-scrollbar { width: 6px; }
.sidebar-nav::-webkit-scrollbar-track { background: transparent; }
.sidebar-nav::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.15);
  border-radius: 10px;
}
.sidebar-nav::-webkit-scrollbar-thumb:hover { background: rgba(0, 0, 0, 0.25); }
</style>
