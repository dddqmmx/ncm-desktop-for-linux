<template>
  <div class="segmented-slider">
    <!-- 滑动背景块 -->
    <div class="slider-bg" :style="bgStyle"></div>

    <!-- 选项按钮 -->
    <button
      v-for="opt in options"
      :key="opt.value"
      :class="{ active: modelValue === opt.value }"
      @click="$emit('update:modelValue', opt.value)"
      type="button"
    >
      {{ opt.label }}
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{
  options: { label: string, value: string }[],
  modelValue: string
}>();

const bgStyle = computed(() => {
  const index = props.options.findIndex(o => o.value === props.modelValue);
  const width = 100 / props.options.length;
  // 使用 padding 的值 (4px) 作为偏移参考
  return {
    width: `calc(${width}% - 8px)`, // 减去左右两侧的内边距差值
    left: `calc(${index * width}% + 4px)`, // 加上起始内边距
  };
});
</script>

<style scoped>
.segmented-slider {
  display: flex;
  position: relative;
  background: rgba(0, 0, 0, 0.06); /* 稍微加深一点点对比度 */
  padding: 4px; /* 外部统一间距 */
  border-radius: 14px;
  height: 40px; /* 固定高度使视觉更统一 */
  align-items: center;
  user-select: none;
}

.slider-bg {
  position: absolute;
  top: 4px;
  bottom: 4px;
  background: white;
  border-radius: 10px;
  /* 更精致的层级阴影 */
  box-shadow:
    0 2px 4px rgba(0, 0, 0, 0.05),
    0 1px 2px rgba(0, 0, 0, 0.1);
  transition: all 0.4s cubic-bezier(0.18, 0.89, 0.32, 1.1); /* 带有弹性感的过渡 */
  z-index: 0;
}

.segmented-slider button {
  flex: 1;
  height: 100%;
  border: none;
  background: none;
  padding: 0 12px;
  font-size: 13px;
  font-weight: 600; /* 600比700在小字号下更显精致 */
  color: rgba(0, 0, 0, 0.45);
  cursor: pointer;
  z-index: 1;
  transition: all 0.3s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  white-space: nowrap;
}

/* 鼠标悬停在非激活项上的反馈 */
.segmented-slider button:not(.active):hover {
  color: rgba(0, 0, 0, 0.7);
}

.segmented-slider button.active {
  color: #111;
}

/* 简单的点击缩放反馈 */
.segmented-slider button:active {
  transform: scale(0.96);
}
</style>
