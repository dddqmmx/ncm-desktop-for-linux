<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { VueDraggable } from 'vue-draggable-plus'
import AppIcon from '@renderer/components/common/AppIcon.vue'
import SettingGroup from '@renderer/components/settings/SettingGroup.vue'
import SettingRow from '@renderer/components/settings/SettingRow.vue'
import SegmentedSlider from '@renderer/components/common/SegmentedSlider.vue'
import {
  useConfigStore,
  type FullscreenAuxButtonId,
  type FullscreenAuxButtonState
} from '@renderer/stores/configStore'

const configStore = useConfigStore()
const { theme, acrylic, accentColor } = storeToRefs(configStore)

const AUX_META: Record<FullscreenAuxButtonId, { label: string; icon: string }> = {
  favorite: { label: '喜欢', icon: 'heart' },
  comment: { label: '评论', icon: 'comment' },
  playMode: { label: '播放顺序', icon: 'loop' },
  volume: { label: '音量', icon: 'volume' }
}

// 拖拽松手后浏览器仍会补发一次 click，若直接当成点击就会顺手把按钮藏掉。
// Sortable 自己会在 document 捕获阶段吞掉那一次 click（我们的处理器根本收不到），
// 因此用一个时间窗而不是布尔标记：既挡住拖拽后紧跟着的那次 click，也不会
// 吞掉用户稍后真正的点击。
const DRAG_CLICK_SUPPRESS_MS = 250
let suppressClickUntil = 0

const handleDragStart = (): void => {
  suppressClickUntil = performance.now() + DRAG_CLICK_SUPPRESS_MS
}

const handleDragEnd = (): void => {
  suppressClickUntil = performance.now() + DRAG_CLICK_SUPPRESS_MS
}

const toggleAuxVisible = (item: FullscreenAuxButtonState): void => {
  if (performance.now() < suppressClickUntil) return
  configStore.setFullscreenAuxVisible(item.id, !item.visible)
}

// 两侧共用一个 group：拖到另一侧时排序库会从源列表删掉、往目标列表插入
const AUX_DRAG_GROUP = { name: 'fullscreen-aux', pull: true, put: true }
const auxTitle = (item: FullscreenAuxButtonState): string =>
  `${AUX_META[item.id].label}：${item.visible ? '点击隐藏' : '点击显示'}`
</script>

<template>
  <div class="settings-tab">
    <SettingGroup title="色彩模式" no-card>
      <SegmentedSlider
        v-model="theme"
        :options="[
          { label: '浅色模式', value: 'light' },
          { label: '深色模式', value: 'dark' },
          { label: '跟随系统', value: 'adaptive' }
        ]"
      />
    </SettingGroup>

    <SettingGroup title="视觉效果">
      <SettingRow title="亚克力 / 云母效果" description="启用窗口半透明磨砂背景">
        <input v-model="acrylic" type="checkbox" class="modern-switch" />
      </SettingRow>
      <SettingRow title="强调色" description="用于后续主题扩展和重点控件着色">
        <input v-model="accentColor" type="color" class="color-picker" />
      </SettingRow>
    </SettingGroup>

    <SettingGroup title="全屏播放页">
      <div class="aux-editor">
        <div class="aux-row" role="group" aria-label="全屏播放页按钮布局预览">
          <VueDraggable
            v-model="configStore.fullscreenAuxLeft"
            :group="AUX_DRAG_GROUP"
            :animation="160"
            ghost-class="aux-ghost"
            class="aux-dock aux-dock-left"
            @start="handleDragStart"
            @end="handleDragEnd"
          >
            <button
              v-for="item in configStore.fullscreenAuxLeft"
              :key="item.id"
              type="button"
              class="aux-icon"
              :class="{ hidden: !item.visible }"
              :title="auxTitle(item)"
              :aria-pressed="item.visible"
              @click="toggleAuxVisible(item)"
            >
              <AppIcon :name="AUX_META[item.id].icon" :size="18" />
            </button>
          </VueDraggable>

          <div class="aux-transport" aria-hidden="true">
            <AppIcon name="prev" :size="20" class="aux-transport-icon" />
            <span class="aux-transport-play"><AppIcon name="play" :size="18" /></span>
            <AppIcon name="next" :size="20" class="aux-transport-icon" />
          </div>

          <VueDraggable
            v-model="configStore.fullscreenAuxRight"
            :group="AUX_DRAG_GROUP"
            :animation="160"
            ghost-class="aux-ghost"
            class="aux-dock aux-dock-right"
            @start="handleDragStart"
            @end="handleDragEnd"
          >
            <button
              v-for="item in configStore.fullscreenAuxRight"
              :key="item.id"
              type="button"
              class="aux-icon"
              :class="{ hidden: !item.visible }"
              :title="auxTitle(item)"
              :aria-pressed="item.visible"
              @click="toggleAuxVisible(item)"
            >
              <AppIcon :name="AUX_META[item.id].icon" :size="18" />
            </button>
          </VueDraggable>
        </div>
        <p class="aux-hint">
          拖动按钮可跨左右两侧移动并调整顺序，点击按钮切换显示 / 隐藏；虚线圈表示当前已隐藏。
          音量按钮仅在开启软件音量后出现。
        </p>
      </div>
    </SettingGroup>
  </div>
</template>

<style scoped>
.aux-editor {
  padding: 18px 0 6px;
}

.aux-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 20px;
  padding: 18px 20px;
  border-radius: 18px;
  background: var(--sys-bg-muted);
  border: 1px solid var(--sys-border);
}

.aux-dock {
  display: flex;
  align-items: center;
  gap: 14px;
  /* 空的时候也要留出可放置的落点，否则按钮拖不出去 */
  min-height: 40px;
  min-width: 40px;
}

.aux-dock-left {
  justify-content: flex-end;
}

.aux-dock-right {
  justify-content: flex-start;
}

.aux-icon {
  width: 34px;
  height: 34px;
  flex-shrink: 0;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  border: 1px solid transparent;
  background: none;
  color: var(--sys-text);
  cursor: grab;
  transition:
    opacity 0.2s ease,
    border-color 0.2s ease,
    background 0.2s ease;
}

.aux-icon:hover {
  background: var(--sys-control-hover);
}

.aux-icon:active {
  cursor: grabbing;
}

.aux-icon.hidden {
  opacity: 0.34;
  border: 1px dashed var(--sys-border-strong);
}

.aux-ghost {
  opacity: 0.3;
}

.aux-transport {
  display: flex;
  align-items: center;
  gap: 18px;
  color: var(--sys-text);
}

.aux-transport-icon {
  opacity: 0.55;
}

.aux-transport-play {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: var(--sys-text);
  color: var(--sys-bg);
}

.aux-hint {
  margin: 12px 0 10px;
  padding-left: 4px;
  font-size: 11px;
  line-height: 1.6;
  color: var(--sys-text-tertiary);
}

@media (max-width: 860px) {
  .aux-row {
    grid-template-columns: 1fr;
    gap: 14px;
  }

  .aux-dock-left,
  .aux-dock-right {
    justify-content: center;
  }

  .aux-transport {
    justify-content: center;
  }
}
</style>
