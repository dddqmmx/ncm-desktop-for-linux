<script setup lang="ts">
import SideNavBar from '@renderer/components/navigation/SideNavBar.vue'
import MusicPlayer from '@renderer/components/player/PlayerBar.vue'
import { onMounted } from 'vue'
import { useUserStore } from '@renderer/stores/userStore'
import PlayerFullscreen from '@renderer/components/player/PlayerFullscreen.vue'
import { usePlayerStore } from '@renderer/stores/playerStore'
import { useFavoriteStore } from '@renderer/stores/favoriteStore'
import { ref, watch } from 'vue'

const userStore = useUserStore()
const playerStore = usePlayerStore()
const favoriteStore = useFavoriteStore()

const isBackgroundHidden = ref(false)
let backgroundHideTimer: number | undefined = undefined

// 监听全屏状态：
// 进入全屏时：延迟 350ms (等待 player-fade 动画结束) 后隐藏底层，节省 GPU 渲染。
// 退出全屏时：立即显示底层，让全屏淡出动画平滑过渡。
watch(
  () => playerStore.isFullScreen,
  (isFull) => {
    if (backgroundHideTimer) {
      window.clearTimeout(backgroundHideTimer)
      backgroundHideTimer = undefined
    }

    if (isFull) {
      backgroundHideTimer = window.setTimeout(() => {
        isBackgroundHidden.value = true
      }, 350)
    } else {
      isBackgroundHidden.value = false
    }
  }
)

onMounted(async () => {
  await userStore.getUserAccount()
  await favoriteStore.fetchFavoriteSongs()
})
</script>

<template>
  <div class="app-background">
    <div class="app-layout">
      <!-- v-show 隐藏底层，避免在全屏时消耗 GPU 进行无意义的重绘 -->
      <div v-show="!isBackgroundHidden" class="app-layout" style="flex: 1;">
        <!-- 左侧导航组件 -->
        <SideNavBar />

        <!-- 右侧内容区 (保留布局结构) -->
        <div class="content-area-blended">
          <RouterView />

          <!-- 底部播放器组件 -->
          <MusicPlayer />
        </div>
      </div>

      <!-- 全屏播放器挂载点 -->
      <div class="fullscreen-container" v-if="playerStore.isFullScreen || isBackgroundHidden">
        <Transition name="player-fade" appear>
          <PlayerFullscreen v-if="playerStore.isFullScreen" />
        </Transition>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* === 布局与背景 === */
.app-background {
  width: 100vw;
  height: 100vh;
  background:
    radial-gradient(circle at top left, var(--theme-color-soft), transparent 32%), var(--sys-bg);
  position: relative;
  overflow: hidden;
  font-family:
    'SF Pro Text',
    'SF Pro Display',
    -apple-system,
    BlinkMacSystemFont,
    'Segoe UI',
    sans-serif;
  color: var(--sys-text);
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

.fullscreen-container {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  z-index: 10;
  pointer-events: none;
}
.fullscreen-container > * {
  pointer-events: auto;
}

.scrollable-content {
  flex: 1;
  overflow-y: auto;
  padding: 20px 16px 0 10px;
  padding-bottom: 0;
}

.player-fade-enter-active,
.player-fade-leave-active {
  transition:
    opacity 0.35s ease,
    transform 0.35s ease;
}

.player-fade-enter-from,
.player-fade-leave-to {
  opacity: 0;
  transform: translateY(20px);
}
</style>
