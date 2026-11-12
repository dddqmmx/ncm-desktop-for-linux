<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import VolumeRow from '@renderer/components/common/VolumeRow.vue'
import { isLocalSong, usePlayerStore } from '@renderer/stores/playerStore'
import { useFavoriteStore } from '@renderer/stores/favoriteStore'
import type { FullscreenAuxButtonId } from '@renderer/stores/configStore'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

const props = defineProps<{
  id: FullscreenAuxButtonId
  /** 评论面板当前是否展开，用于按钮激活态 */
  commentOpen: boolean
}>()

const emit = defineEmits<{
  (e: 'toggle-comment'): void
}>()

const playerStore = usePlayerStore()
const favoriteStore = useFavoriteStore()

const isCurrentSongLocal = computed(() => isLocalSong(playerStore.currentSong))
const isCurrentSongFavorite = computed(() => favoriteStore.isFavorite(playerStore.currentSongId))

const favoriteTitle = computed(() => {
  if (isCurrentSongLocal.value) return '本地音乐'
  return isCurrentSongFavorite.value ? '取消喜欢' : '喜欢'
})

const toggleCurrentFavorite = (): void => {
  if (!playerStore.currentSong || isCurrentSongLocal.value) return
  void favoriteStore.toggleFavorite(playerStore.currentSong)
}

const canShowComments = computed(
  () => Boolean(playerStore.currentSong) && !isCurrentSongLocal.value
)
const commentTitle = computed(() => {
  if (isCurrentSongLocal.value) return '本地音乐暂无评论'
  if (!playerStore.currentSong) return '暂无播放歌曲'
  return props.commentOpen ? '返回歌词' : '查看评论'
})

const playModeIcon = computed(() => {
  if (playerStore.playMode === 'random') return 'shuffle'
  if (playerStore.playMode === 'single') return 'single'
  return 'loop'
})

const playModeTitle = computed(() => {
  if (playerStore.playMode === 'random') return '随机播放'
  if (playerStore.playMode === 'single') return '单曲循环'
  return '列表循环'
})

// --- 软件音量浮层：点击外部收起 ---
const isVolumePanelVisible = ref(false)
const volumeControlRef = ref<HTMLElement | null>(null)

const volumeTitle = computed(() => {
  if (playerStore.sessionVolumeLocked) {
    return 'BitPerfect 直通中，音量将在下一曲生效'
  }
  return `音量 ${Math.round(playerStore.volumePercent)}%`
})

const toggleVolumePanel = (): void => {
  isVolumePanelVisible.value = !isVolumePanelVisible.value
}

const handleDocumentClick = (event: MouseEvent): void => {
  if (!isVolumePanelVisible.value) return
  if (volumeControlRef.value && !volumeControlRef.value.contains(event.target as Node)) {
    isVolumePanelVisible.value = false
  }
}

onMounted(() => {
  if (props.id !== 'volume') return
  document.addEventListener('click', handleDocumentClick)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', handleDocumentClick)
})
</script>

<template>
  <button
    v-if="id === 'favorite'"
    type="button"
    class="aux-btn"
    :class="{ active: isCurrentSongFavorite }"
    :disabled="!playerStore.currentSong || isCurrentSongLocal"
    :title="favoriteTitle"
    @click="toggleCurrentFavorite"
  >
    <AppIcon :name="isCurrentSongFavorite ? 'heart-fill' : 'heart'" :size="20" />
  </button>

  <button
    v-else-if="id === 'comment'"
    type="button"
    class="aux-btn"
    :class="{ active: commentOpen }"
    :disabled="!canShowComments"
    :title="commentTitle"
    :aria-label="commentTitle"
    @click="emit('toggle-comment')"
  >
    <AppIcon name="comment" :size="20" />
  </button>

  <button
    v-else-if="id === 'playMode'"
    type="button"
    class="aux-btn"
    :title="playModeTitle"
    @click="playerStore.togglePlayMode()"
  >
    <AppIcon :name="playModeIcon" :size="20" />
  </button>

  <div v-else ref="volumeControlRef" class="volume-control">
    <Transition name="volume-fade">
      <div v-if="isVolumePanelVisible" class="volume-popup">
        <VolumeRow
          v-model="playerStore.volumePercent"
          :disabled="playerStore.sessionVolumeLocked"
          locked-hint="BitPerfect 直通中，音量将在下一曲生效"
        />
      </div>
    </Transition>
    <button
      type="button"
      class="aux-btn"
      :class="{ active: isVolumePanelVisible }"
      :title="volumeTitle"
      @click.stop="toggleVolumePanel"
    >
      <AppIcon :name="playerStore.volumePercent <= 0 ? 'volume-mute' : 'volume'" :size="20" />
    </button>
  </div>
</template>

<style scoped>
/* 按钮样式自带一份：全屏页与本组件各写一套样式表，耦合在父级的
   scoped 规则上会在任何一次父级样式重排里悄悄失效。 */
.aux-btn {
  background: none;
  border: none;
  color: var(--text-color);
  opacity: 0.55;
  cursor: pointer;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: opacity 0.2s ease;
  -webkit-app-region: no-drag;
}

.aux-btn:hover {
  opacity: 1;
}

.aux-btn.active {
  opacity: 1;
}

.aux-btn:disabled {
  opacity: 0.25;
  cursor: default;
}

/* 音量控制（按钮与走带键同款风格，弹层对齐主题色变量） */
.volume-control {
  position: relative;
  display: flex;
  align-items: center;
  -webkit-app-region: no-drag;
}

.volume-popup {
  position: absolute;
  bottom: calc(100% + 14px);
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  padding: 10px 14px;
  background: var(--contrast-color);
  border: 0.5px solid rgba(255, 255, 255, 0.12);
  border-radius: 14px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  white-space: nowrap;
  -webkit-app-region: no-drag;
}

.volume-popup :deep(.volume-value) {
  color: var(--text-color);
}

.volume-fade-enter-active,
.volume-fade-leave-active {
  transition:
    opacity 0.2s ease,
    transform 0.2s ease;
}

.volume-fade-enter-from,
.volume-fade-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(6px);
}
</style>
