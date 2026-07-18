<script setup lang="ts">
import AppIcon from './AppIcon.vue'
defineProps<{
  title?: string
  subtitle?: string
  desc?: string
  type?: string
  bgClass?: string
  image?: string | null
  logo?: string
  isFirst?: boolean
}>()
</script>

<template>
  <div
    class="large-card"
    :class="bgClass"
    :style="image ? { backgroundImage: `url(${image})` } : {}"
  >
    <div class="card-overlay"></div>
    <div class="card-content">
      <div class="card-top">
        <span v-if="logo" class="logo-text">{{ logo }}</span>
      </div>
      <div class="card-center">
        <h3 v-if="title">{{ title }}</h3>
        <h4 v-if="subtitle">{{ subtitle }}</h4>
      </div>
      <div class="card-bottom">
        <p v-if="desc">{{ desc }}</p>
        <div v-if="type === 'station'" class="station-tag">{{ title }}</div>
      </div>
      <!-- 只有第一个卡片显示播放按钮 -->
      <button v-if="isFirst" class="play-btn-circle">
        <AppIcon name="play" :size="20" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.large-card {
  aspect-ratio: 1 / 1.25;
  border-radius: 18px;
  position: relative;
  background-color: var(--sys-bg-muted);
  background-size: cover;
  background-position: center;
  box-shadow: var(--sys-shadow-soft);
  overflow: visible; /* 改为 visible，或者删除这一行 */
  transition:
    transform 0.25s,
    box-shadow 0.25s; /* 增加阴影过渡动画 */
}
.card-overlay,
.card-content {
  border-radius: 18px;
}
.large-card:hover {
  transform: translateY(-4px);
  box-shadow: var(--sys-shadow-elevated);
}
.card-overlay {
  position: absolute;
  inset: 0;
  background: linear-gradient(to bottom, rgba(0, 0, 0, 0) 30%, rgba(0, 0, 0, 0.7));
}
.bg-gradient-orange {
  background: linear-gradient(135deg, var(--theme-color) 0%, var(--theme-color-tint) 100%);
}
.bg-gradient-pink {
  background: linear-gradient(45deg, var(--theme-color-strong) 0%, var(--theme-color-tint) 100%);
}
.card-content {
  position: absolute;
  inset: 0;
  padding: 24px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  color: white;
  z-index: 2;
}
.logo-text {
  font-size: 11px;
  font-weight: 700;
  opacity: 0.9;
}
.card-center h3 {
  font-size: 28px;
  line-height: 1.1;
  font-weight: 800;
  margin: 0;
  display: -webkit-box;
  line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-overflow: ellipsis;
  word-break: break-all;
}
.card-center h4 {
  font-size: 28px;
  font-weight: 300;
  margin: 0;
  opacity: 0.95;
}
.card-bottom p {
  font-size: 13px;
  font-weight: 500;
  opacity: 0.9;
  line-height: 1.4;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  margin: 0;
}
.play-btn-circle {
  position: absolute;
  bottom: 20px;
  right: 20px;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.25);
  backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.4);
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}
</style>
