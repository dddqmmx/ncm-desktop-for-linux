<script setup lang="ts">
import { computed, ref, type Component } from 'vue'
import '@renderer/assets/settings.css'
import SettingsSidebar from './SettingsSidebar.vue'
import GeneralSettingsTab from './settings/GeneralSettingsTab.vue'
import AudioSettingsTab from './settings/AudioSettingsTab.vue'
import AppearanceSettingsTab from './settings/AppearanceSettingsTab.vue'
import LibrarySettingsTab from './settings/LibrarySettingsTab.vue'
import ShortcutsSettingsTab from './settings/ShortcutsSettingsTab.vue'
import AboutSettingsTab from './settings/AboutSettingsTab.vue'

type SettingsTabId = 'general' | 'audio' | 'appearance' | 'library' | 'shortcuts' | 'about'

type SettingsTab = {
  id: SettingsTabId
  name: string
  icon: string
}

const emit = defineEmits<{ close: [] }>()

const tabs: SettingsTab[] = [
  {
    id: 'general',
    name: '通用',
    icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4'
  },
  {
    id: 'audio',
    name: '音频输出',
    icon: 'M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z'
  },
  {
    id: 'appearance',
    name: '外观个性化',
    icon: 'M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-3'
  },
  {
    id: 'library',
    name: '曲库管理',
    icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'
  },
  {
    id: 'shortcuts',
    name: '快捷键',
    icon: 'M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25'
  },
  {
    id: 'about',
    name: '关于软件',
    icon: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z'
  }
]

const tabComponents: Record<SettingsTabId, Component> = {
  general: GeneralSettingsTab,
  audio: AudioSettingsTab,
  appearance: AppearanceSettingsTab,
  library: LibrarySettingsTab,
  shortcuts: ShortcutsSettingsTab,
  about: AboutSettingsTab
}

const activeTab = ref<SettingsTabId>('general')

const activeTabTitle = computed(() => {
  return tabs.find((tab) => tab.id === activeTab.value)?.name ?? ''
})

const activeTabComponent = computed(() => tabComponents[activeTab.value])
</script>

<template>
  <div class="settings-mask" @click.self="emit('close')">
    <div class="settings-window glass-morphism-heavy">
      <SettingsSidebar v-model:active-tab="activeTab" :tabs="tabs" />

      <main class="settings-main">
        <header class="main-header">
          <h2>{{ activeTabTitle }}</h2>
          <button class="close-btn" @click="emit('close')">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
            >
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </header>

        <div class="content-scroll">
          <component :is="activeTabComponent" />
        </div>
      </main>
    </div>
  </div>
</template>

<style scoped>

.settings-mask {
  position: fixed;
  inset: 0;
  z-index: 2000;
  background: rgba(0, 0, 0, 0.15);
  display: flex;
  align-items: center;
  justify-content: center;
  -webkit-app-region: no-drag;
}

.settings-window {
  width: 960px;
  height: 640px;
  border-radius: 32px;
  display: flex;
  overflow: hidden;
  animation: modalScaleUp 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  -webkit-app-region: no-drag;
}

.glass-morphism-heavy {
  background: rgba(245, 245, 247, 0.75);
  backdrop-filter: blur(40px) saturate(160%);
  -webkit-backdrop-filter: blur(40px) saturate(160%);
  border: 1px solid rgba(255, 255, 255, 0.5);
  box-shadow: 0 30px 80px rgba(0, 0, 0, 0.15);
}

.settings-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: rgba(255, 255, 255, 0.2);
}

.main-header {
  padding: 32px 48px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.main-header h2 {
  font-size: 24px;
  font-weight: 800;
  margin: 0;
}

.close-btn {
  background: none;
  border: none;
  color: rgba(0, 0, 0, 0.3);
  cursor: pointer;
}

.content-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 0 48px 48px;
}

@keyframes modalScaleUp {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(20px);
  }

  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

.content-scroll::-webkit-scrollbar {
  width: 6px;
}

.content-scroll::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.1);
  border-radius: 10px;
}

.content-scroll::-webkit-scrollbar-thumb:hover {
  background: rgba(0, 0, 0, 0.25);
}
</style>
