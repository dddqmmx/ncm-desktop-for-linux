<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'

const props = defineProps<{
  songId?: number
  currentTime: number // 假设是毫秒
}>()

// --- 配置常量 ---
const LYRIC_OFFSET_MS = 200 // 提前 200ms 触发高亮和滚动，补偿人眼感知和动画启动

interface LyricLine {
  time: number
  text: string
}

const loading = ref(false)
const lyrics = ref<LyricLine[]>([])
const lineRefs = ref<HTMLElement[]>([])

// 解析逻辑保持不变...
const parseLyric = (lrcString: string): LyricLine[] => {
  const lines = lrcString.split('\n')
  const result: LyricLine[] = []
  const timeExp = /\[(\d{2}):(\d{2})\.(\d{2,3})\]/
  lines.forEach(line => {
    const match = timeExp.exec(line)
    if (match) {
      const m = parseInt(match[1]); const s = parseInt(match[2]); const ms = parseInt(match[3])
      const time = m * 60 + s + (ms > 99 ? ms / 1000 : ms / 100)
      const text = line.replace(timeExp, '').trim()
      if (text) result.push({ time, text })
    }
  })
  return result.sort((a, b) => a.time - b.time)
}

const fetchLyrics = async (id: number) => {
  try {
    loading.value = true
    const res = await window.api.lyric({ id }) as any
    if (res.body?.lrc?.lyric) {
      lyrics.value = parseLyric(res.body.lrc.lyric)
    } else {
      lyrics.value = [{ time: 0, text: '暂无歌词' }]
    }
  } catch (error) {
    lyrics.value = [{ time: 0, text: '歌词加载失败' }]
  } finally {
    loading.value = false
  }
}

watch(() => props.songId, (newId) => {
  if (newId) fetchLyrics(newId)
}, { immediate: true })

// --- 逻辑优化：提前量计算 ---
const currentLyricIndex = computed(() => {
  // 加上偏移量，让 index 的切换提前发生
  const adjustedTime = (props.currentTime + LYRIC_OFFSET_MS) / 1000
  return lyrics.value.findLastIndex(l => adjustedTime >= l.time)
})

// --- 滚动逻辑 ---
watch(currentLyricIndex, (newIndex) => {
  if (newIndex === -1) return
  nextTick(() => {
    const activeEl = lineRefs.value[newIndex]
    if (activeEl) {
      activeEl.scrollIntoView({
        behavior: 'smooth',
        block: 'center',
      })
    }
  })
})
</script>

<template>
  <section class="lyrics-panel">
    <div v-if="loading" class="lyric-status">加载中...</div>
    <div v-else class="lyrics-scroll-container">
      <div
        v-for="(line, index) in lyrics"
        :key="index"
        :ref="(el) => { if (el) lineRefs[index] = el as HTMLElement }"
        class="lyric-line"
        :class="{ 'active': index === currentLyricIndex }"
      >
        <span class="lyric-text">{{ line.text }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.lyrics-panel {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  /* 增加遮罩的平滑度 */
  mask-image: linear-gradient(
    to bottom,
    transparent 0%,
    black 20%,
    black 80%,
    transparent 100%
  );
  -webkit-mask-image: linear-gradient(
    to bottom,
    transparent 0%,
    black 20%,
    black 80%,
    transparent 100%
  );
}

.lyrics-scroll-container {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden; /* 严禁左右滚动 */
  padding: 50% 40px; /* 左右 Padding 必须足够大，防止 scale 后的文字被 mask 截断 */
  scroll-behavior: smooth;
  scrollbar-width: none;
  /* 启用硬件加速 */
  will-change: scroll-position;
}
.lyrics-scroll-container::-webkit-scrollbar { display: none; }

.lyric-line {
  /* 使用 margin 而不是 padding 来控制行间距，这样 scale 不会影响行高布局 */
  margin: 12px 0;
  padding: 8px 0;
  line-height: 1.4;
  text-align: center;

  /* 优化过渡曲线：out-expo 风格，开始快，结束慢，视觉上更灵敏 */
  transition:
    transform 0.4s cubic-bezier(0.23, 1, 0.32, 1),
    opacity 0.4s cubic-bezier(0.23, 1, 0.32, 1),
    filter 0.4s ease;

  opacity: 0.3;
  color: rgba(255, 255, 255, 0.9);
  font-size: 26px;
  font-weight: 500;

  /* 关键：防止抖动的核心属性 */
  will-change: transform, opacity;
  backface-visibility: hidden;
  transform-origin: center center;
  filter: blur(0.5px); /* 未激活时轻微模糊，增加层次感 */
}

.lyric-line.active {
  opacity: 1;
  font-weight: 700;
  /* 缩放控制在 1.1 以内，配合 padding 40px 绝不会截断 */
  transform: scale(1.1);
  filter: blur(0);
  color: #ffffff;
  text-shadow: 0 0 18px rgba(255, 255, 255, 0.4);
}

.lyric-text {
  display: inline-block;
  /* 限制宽度防止溢出容器 */
  max-width: 100%;
  word-wrap: break-word;
  white-space: pre-wrap;
}

.lyric-status {
  height: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
  font-size: 18px;
  color: rgba(255, 255, 255, 0.5);
}
</style>
