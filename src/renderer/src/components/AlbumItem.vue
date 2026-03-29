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
      <img :src="cover" :alt="title" loading="lazy" />
      <div class="album-overlay"></div>
    </div>
    <div class="album-info">
      <div class="album-title">{{ title }}</div>
      <div class="album-artist">{{ artist }}</div>
    </div>
  </div>
</template>

<style scoped>
.album-item {
  width: 176px;
  flex-shrink: 0;
  cursor: pointer;
  transition: transform 0.3s ease;
}

.album-item:hover {
  transform: translateY(-4px);
}

.album-cover {
  position: relative;
  width: 100%;
  aspect-ratio: 1;
  border-radius: 28px;
  overflow: hidden;
  margin-bottom: 14px;
  background: linear-gradient(145deg, rgba(255, 255, 255, 0.8), rgba(214, 225, 255, 0.46));
  border: 1px solid rgba(255, 255, 255, 0.72);
  box-shadow:
    0 20px 44px rgba(38, 53, 84, 0.12),
    inset 0 1px 0 rgba(255, 255, 255, 0.85);
}

.album-gradient,
.album-cover img,
.album-overlay,
.album-sheen {
  position: absolute;
  inset: 0;
}

.album-gradient {
  background:
    linear-gradient(135deg, rgba(236, 240, 255, 0.95), rgba(248, 237, 229, 0.78)),
    linear-gradient(180deg, rgba(180, 197, 232, 0.34), rgba(255, 255, 255, 0));
}

.album-cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  transition: transform 0.35s ease;
}

.album-item:hover .album-cover img {
  transform: scale(1.06);
}

.album-overlay {
  background: linear-gradient(180deg, rgba(16, 24, 40, 0) 42%, rgba(16, 24, 40, 0.16) 100%);
}

.album-sheen {
  background:
    radial-gradient(circle at top left, rgba(255, 255, 255, 0.82), transparent 40%),
    radial-gradient(circle at bottom right, rgba(255, 255, 255, 0.22), transparent 30%);
  mix-blend-mode: screen;
}

.album-badge,
.album-play-chip {
  position: absolute;
  z-index: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: rgba(255, 255, 255, 0.22);
  border: 1px solid rgba(255, 255, 255, 0.46);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.42);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  color: white;
}

.album-badge {
  top: 12px;
  left: 12px;
  min-height: 28px;
  padding: 0 12px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
}

.album-play-chip {
  right: 12px;
  bottom: 12px;
  width: 38px;
  height: 38px;
  border-radius: 50%;
  opacity: 0;
  transform: translateY(6px);
  transition:
    opacity 0.25s ease,
    transform 0.25s ease;
}

.album-item:hover .album-play-chip {
  opacity: 1;
  transform: translateY(0);
}

.album-info {
  padding: 0 6px;
}

.album-title {
  font-size: 15px;
  font-weight: 700;
  line-height: 1.35;
  color: #152033;
  letter-spacing: -0.02em;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.album-artist {
  margin-top: 6px;
  font-size: 13px;
  line-height: 1.45;
  color: rgba(21, 32, 51, 0.62);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
</style>
