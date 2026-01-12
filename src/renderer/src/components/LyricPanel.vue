<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'

const props = defineProps<{
  songId?: number
  currentTime: number
}>()

interface LyricLine {
  time: number
  text: string
}

const loading = ref(false)
const lyrics = ref<LyricLine[]>([])
const lyricsContainer = ref<HTMLElement | null>(null)
// 引用所有的歌词行
const lineRefs = ref<HTMLElement[]>([])

// 解析函数保持不变...
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

const currentLyricIndex = computed(() => {
  const currSeconds = (props.currentTime || 0) / 1000
  return lyrics.value.findLastIndex(l => currSeconds >= l.time)
})

// --- 自动滚动逻辑 ---
watch(currentLyricIndex, (newIndex) => {
  if (newIndex === -1) return
  nextTick(() => {
    // 获取当前高亮的 DOM 元素
    const activeEl = lineRefs.value[newIndex]
    if (activeEl) {
      activeEl.scrollIntoView({
        behavior: 'smooth',
        block: 'center', // 将高亮行滚动到容器中心
      })

      
    }
  })
})
</script>

<template>
  <section class="lyrics-panel">
    <div v-if="loading" class="lyric-status">加载中...</div>
    <div v-else class="lyrics-scroll-container" ref="lyricsContainer">
      <!-- 占位，让第一句歌词能滚到中间 -->
      <div class="lyric-spacer"></div>

      <div
        v-for="(line, index) in lyrics"
        :key="index"
        :ref="(el) => { if (el) lineRefs[index] = el as HTMLElement }"
        class="lyric-line"
        :class="{ 'active': index === currentLyricIndex }"
      >
        {{ line.text }}
      </div>

      <!-- 占位，让最后一句歌词能滚到中间 -->
      <div class="lyric-spacer"></div>
    </div>
  </section>
</template>

<style scoped>
.lyrics-panel {
  height: 100%; /* 必须：占据父级 grid/flex 的全高 */
  display: flex;
  flex-direction: column;
  overflow: hidden; /* 防止溢出 */
  mask-image: linear-gradient(to bottom, transparent 0%, black 15%, black 85%, transparent 100%);
}

.lyrics-scroll-container {
  flex: 1;
  overflow-y: auto;
  padding: 0 20px;
  scroll-behavior: smooth;
  /* 隐藏滚动条 */
  scrollbar-width: none;
}
.lyrics-scroll-container::-webkit-scrollbar { display: none; }

.lyric-spacer {
  height: 40%; /* 上下留白，保证歌词能居中 */
}

.lyric-line {
  font-size: 28px;
  font-weight: 700;
  padding: 18px 0;
  line-height: 1.4;
  opacity: 0.3;
  transition: all 0.5s cubic-bezier(0.25, 0.46, 0.45, 0.94);
  transform-origin: left center;
  cursor: default;
}

.lyric-line.active {
  opacity: 1;
  transform: scale(1.08); /* 稍微放大一点 */
  color: #ffffff;
  text-shadow: 0 0 20px rgba(255, 255, 255, 0.3);
}

.lyric-status {
  height: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
  font-size: 20px;
  opacity: 0.6;
}
</style>
