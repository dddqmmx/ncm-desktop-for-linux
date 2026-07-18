<script setup lang="ts">
import { computed } from 'vue'
import { iconSvgMap, type IconName } from '@renderer/assets/icons'

const props = withDefaults(
  defineProps<{
    name: IconName | string
    size?: number | string
    class?: string
  }>(),
  {
    size: 24
  }
)

const svg = computed(() => iconSvgMap[props.name as IconName] ?? '')

const style = computed(() => {
  const size = typeof props.size === 'number' ? `${props.size}px` : props.size
  return {
    width: size,
    height: size
  }
})
</script>

<template>
  <span
    class="app-icon"
    :class="props.class"
    :style="style"
    aria-hidden="true"
    v-html="svg"
  />
</template>

<style scoped>
.app-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  line-height: 0;
  color: inherit;
}

.app-icon :deep(svg) {
  width: 100%;
  height: 100%;
  display: block;
}
</style>
