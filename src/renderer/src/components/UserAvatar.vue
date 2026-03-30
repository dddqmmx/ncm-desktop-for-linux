<script setup lang="ts">
import LazyImage from './LazyImage.vue'

interface Props {
  /** 用户头像 URL (avatarUrl) */
  id: string | number | null | undefined
  /** 用户名 */
  alt?: string
  /** 是否圆角，头像通常是圆的 */
  rounded?: boolean
  /** 尺寸参数 */
  size?: string
}

withDefaults(defineProps<Props>(), {
  rounded: true,
  size: '200y200'
})
</script>

<template>
  <div class="user-avatar-container" :class="{ 'is-rounded': rounded }">
    <LazyImage 
      :id="id" 
      :alt="alt" 
      :rounded="rounded"
      :param="size"
      type="avatar"
    />
  </div>
</template>

<style scoped>
.user-avatar-container {
  width: 100%;
  height: 100%;
  overflow: hidden;
  border-radius: inherit;
}

.is-rounded {
  border-radius: 50%;
}
</style>
