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
import { computed } from 'vue';

const props = defineProps<{ tabs: any[], activeTab: string }>();

const indicatorStyle = computed(() => {
  const index = props.tabs.findIndex(t => t.id === props.activeTab);
  return { transform: `translateY(${index * 52}px)` };
});
</script>

<style scoped>
.settings-sidebar {
  width: 240px;
  border-right: 1px solid rgba(0, 0, 0, 0.05);
  display: flex;
  flex-direction: column;
  padding: 32px 16px;
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
.brand-dot { width: 8px; height: 8px; background: #111; border-radius: 2px; }
.sidebar-nav { position: relative; flex: 1; }
.nav-indicator {
  position: absolute;
  left: 0;
  top: 0;
  width: 100%;
  height: 44px;
  background: rgba(0, 0, 0, 0.04);
  border-radius: 12px;
  transition: transform 0.3s cubic-bezier(0.2, 1, 0.3, 1);
}
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
  color: rgba(0, 0, 0, 0.5);
  cursor: pointer;
  z-index: 1;
}
.nav-item.active { color: #111; }
.nav-icon { width: 18px; height: 18px; }
</style>
