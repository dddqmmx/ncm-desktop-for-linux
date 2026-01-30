<script setup lang="ts">
import SideNavBar from '../components/SideNavBar.vue'
import MusicPlayer from '../components/PlayerBar.vue'
import { onMounted } from 'vue';
import { useUserStore } from '@renderer/stores/userStore';
import PlayerFullscreen from '@renderer/components/PlayerFullscreen.vue';
import { usePlayerStore } from '@renderer/stores/playerStore';
import Settings from '@renderer/components/Settings.vue';
const userStore = useUserStore()
const playerStore = usePlayerStore()

onMounted(async ()=>{
  await userStore.getUserAccount();
  console.log(await window.api.recommend_resource({cookie: userStore.cookie}))
})

</script>

<template>
  <div class="app-background">
    <div class="app-layout">
      <!-- 左侧导航组件 -->
      <SideNavBar />

      <!-- 右侧内容区 (保留布局结构) -->
      <div class="content-area-blended">
        <RouterView />

        <!-- 底部播放器组件 -->
        <MusicPlayer />

        <Transition name="player-fade">
          <PlayerFullscreen v-if="playerStore.isFullScreen"/>
        </Transition>

      </div>

    </div>
  </div>
</template>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&display=swap');

/* === 布局与背景 === */
.app-background {
  width: 100vw;
  height: 100vh;
  background-color: #eff1f5;
  position: relative;
  overflow: hidden;
  font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  color: #1a1a1a;
}

.app-layout {
  position: relative;
  z-index: 1;
  display: flex;
  width: 100%;
  height: 100%;
}

.content-area-blended {
  flex: 1;
  background: transparent;
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
}

.scrollable-content {
  flex: 1;
  overflow-y: auto;
  padding: 20px 16px 0 10px;
  padding-bottom: 0;
}

/* 滚动条美化 */
.scrollable-content::-webkit-scrollbar { width: 8px; }
.scrollable-content::-webkit-scrollbar-thumb { background: rgba(0,0,0,0.1); border-radius: 4px; }
.scrollable-content::-webkit-scrollbar-track { background: transparent; }


.player-fade-enter-active,
.player-fade-leave-active {
  transition: opacity 0.35s ease, transform 0.35s ease;
}

.player-fade-enter-from,
.player-fade-leave-to {
  opacity: 0;
  transform: translateY(20px);
}


</style>
