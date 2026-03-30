<script setup lang="ts">
import LazyImage from './LazyImage.vue'

interface Props {
  /** 歌曲封面 URL (picUrl) */
  id: string | number | null | undefined
  /** 图片描述 */
  alt?: string
  /** 尺寸参数，默认 200y200 */
  size?: string
}

withDefaults(defineProps<Props>(), {
  size: '200y200'
})
</script>

<template>
  <div class="song-cover-container">
    <LazyImage :id="id" :alt="alt" :param="size" type="cover" />
  </div>
</template>

<style scoped>
.song-cover-container {
  width: 100%;
  height: 100%;
  border-radius: inherit;
  overflow: hidden;
}
</style>
