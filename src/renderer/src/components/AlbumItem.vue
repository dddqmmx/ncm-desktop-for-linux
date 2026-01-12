<script setup lang="ts">
defineProps<{
  title: string
  artist: string
  cover: string
}>()
</script>

<template>
  <div class="album-item">
    <div class="album-cover">
      <img :src="cover" :alt="title" loading="lazy">
      <div class="album-overlay"></div>
    </div>
    <div class="album-info">
      <div class="album-title">{{ title }}</div>
      <div class="album-artist">{{ artist }}</div>
    </div>
  </div>
</template>

<style scoped>
.album-item { width: 160px; flex-shrink: 0; cursor: pointer; transition: opacity 0.2s; }
.album-cover { width: 100%; aspect-ratio: 1; border-radius: 10px; overflow: hidden; margin-bottom: 10px; box-shadow: 0 4px 12px rgba(0,0,0,0.06); }
.album-cover img { width: 100%; height: 100%; object-fit: cover; }
.album-title { font-size: 14px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: #111; }
.album-artist { font-size: 13px; color: #666; }
</style>
