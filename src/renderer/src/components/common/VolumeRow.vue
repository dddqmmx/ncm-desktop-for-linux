<script setup lang="ts">
import { computed } from 'vue'
import RangeSlider from './RangeSlider.vue'

/**
 * 应用级软件音量滑块行（滑块 + 百分比标签）。
 *
 * 播放条弹窗、设置页、全屏播放页共用同一份展示与输入逻辑，避免“0% 静音态、
 * min-width 修正”这类显示改动只落在其中一处、不同页面出现同一音量不同显示。
 * 禁用态统一收口在这里：设置层不可调（软件音量关/BitPerfect 开）与
 * 会话层锁定（BitPerfect 开播即冻结）都是禁用，原因由调用方决定。
 */
const props = withDefaults(
  defineProps<{
    /** 音量百分比（0-100），v-model */
    modelValue: number
    /** 禁用滑块（不可调或会话锁定） */
    disabled?: boolean
    /** 滑块轨道宽度，接受任意 CSS 宽度值；默认撑满父容器 */
    width?: string
    /** 锁定提示文案（如“BitPerfect 直通中，下一曲生效”），悬停显示 */
    lockedHint?: string
  }>(),
  {
    disabled: false,
    width: undefined,
    lockedHint: undefined
  }
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: number): void
}>()

const percentLabel = computed(() => `${Math.round(props.modelValue)}%`)
</script>

<template>
  <div class="volume-row" :title="disabled ? lockedHint : undefined">
    <div class="volume-slider" :style="width ? { width } : undefined">
      <RangeSlider
        :model-value="modelValue"
        :min="0"
        :max="100"
        :step="1"
        :disabled="disabled"
        @update:model-value="emit('update:modelValue', $event)"
      />
    </div>
    <span class="volume-value" :class="{ muted: disabled }">{{ percentLabel }}</span>
  </div>
</template>

<style scoped>
.volume-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.volume-slider {
  width: 120px;
}

.volume-value {
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  color: var(--sys-text-secondary);
  min-width: 36px;
  text-align: right;
  white-space: nowrap;
}

.volume-value.muted {
  opacity: 0.5;
}
</style>
