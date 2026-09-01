<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 允许 string：与 v-model.number 的数字输入框共享 ref 时，清空输入框会短暂出现 '' */
    modelValue: number | string
    min?: number
    max?: number
    step?: number
    disabled?: boolean
    /** 透传给内部 input，供 <label for> 关联 */
    inputId?: string
  }>(),
  {
    min: 0,
    max: 100,
    step: 1,
    disabled: false
  }
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: number): void
}>()

const fillPercent = computed(() => {
  const range = props.max - props.min
  if (range <= 0) return 0
  const value = Number(props.modelValue)
  if (!Number.isFinite(value)) return 0
  return Math.max(0, Math.min(100, ((value - props.min) / range) * 100))
})

const handleInput = (event: Event): void => {
  emit('update:modelValue', Number((event.target as HTMLInputElement).value))
}
</script>

<template>
  <div class="custom-slider-wrapper" :class="{ disabled }">
    <div class="custom-slider-track">
      <div class="custom-slider-fill" :style="{ width: fillPercent + '%' }"></div>
    </div>
    <input
      :id="inputId"
      class="custom-slider-input"
      type="range"
      :min="min"
      :max="max"
      :step="step"
      :value="modelValue"
      :disabled="disabled"
      @input="handleInput"
    />
  </div>
</template>

<style scoped>
/* DIV 模拟视觉 + 透明 input 承接交互，与缓存设置的滑块同一套样式 */
.custom-slider-wrapper {
  position: relative;
  width: 100%;
  height: 20px; /* 点击热区高度，方便鼠标/手指操作 */
  display: flex;
  align-items: center;
}

.custom-slider-track {
  width: 100%;
  height: 6px;
  background: var(--sys-control);
  border-radius: 999px;
  overflow: hidden;
  pointer-events: none; /* 让鼠标事件穿透给底下的 input */
}

.custom-slider-fill {
  height: 100%;
  background: var(--theme-color);
  border-radius: inherit;
  /* 不加过渡效果，保证拖拽时极致跟手 */
}

.custom-slider-input {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  margin: 0;
  opacity: 0; /* 原生 input 完全透明化 */
  cursor: pointer;
}

.custom-slider-wrapper.disabled {
  opacity: 0.4;
}

.custom-slider-input:disabled {
  cursor: not-allowed;
}
</style>
