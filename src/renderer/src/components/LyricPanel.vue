<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import type { Lyric } from '@renderer/types/lyric'

const props = defineProps<{
  songId?: number
  currentTime: number // 假设是毫秒
  isDark: boolean
}>()

// --- 配置常量 ---
const LYRIC_OFFSET_MS = 400 // 提前 400ms 触发高亮和滚动，补偿人眼感知和动画启动
const LYRIC_PRONUNCIATION_VISIBLE_KEY = 'lyric:showPronunciation'
const LYRIC_TRANSLATION_VISIBLE_KEY = 'lyric:showTranslation'

interface LyricLine {
  time: number
  text: string
  translation?: string
  pronunciation?: string
}

const loading = ref(false)
const lyrics = ref<LyricLine[]>([])
const lineRefs = ref<HTMLElement[]>([])
const scrollContainerRef = ref<HTMLElement | null>(null)
const showPronunciation = ref(localStorage.getItem(LYRIC_PRONUNCIATION_VISIBLE_KEY) !== 'false')
const showTranslation = ref(localStorage.getItem(LYRIC_TRANSLATION_VISIBLE_KEY) !== 'false')
const isUserScrolling = ref(false)
let userScrollTimer: number | undefined
let lyricRequestId = 0

const hasPronunciation = computed(() => lyrics.value.some((line) => !!line.pronunciation))
const hasTranslation = computed(() => lyrics.value.some((line) => !!line.translation))

// --- 样式计算：优雅的主题切换 ---
const themeVars = computed(() => {
  const { isDark } = props
  return {
    '--lrc-text-color': isDark ? 'rgba(255, 255, 255, 0.9)' : 'rgba(0, 0, 0, 0.9)',
    '--lrc-text-active-color': isDark ? '#ffffff' : '#000000',
    '--lrc-text-shadow': isDark ? 'rgba(255, 255, 255, 0.4)' : 'rgba(0, 0, 0, 0.1)',
    '--lrc-status-color': isDark ? 'rgba(255, 255, 255, 0.5)' : 'rgba(0, 0, 0, 0.5)'
  }
})

// 解析逻辑
const parseTimedLines = (lrcString: string): LyricLine[] => {
  const lines = lrcString.split('\n')
  const result: LyricLine[] = []
  const timeExp = /\[(\d{2}):(\d{2})\.(\d{2,3})\]/
  lines.forEach((line) => {
    const match = timeExp.exec(line)
    if (match) {
      const m = parseInt(match[1])
      const s = parseInt(match[2])
      const ms = parseInt(match[3])
      const time = m * 60 + s + (ms > 99 ? ms / 1000 : ms / 100)
      const text = line.replace(timeExp, '').trim()
      if (text) result.push({ time, text })
    }
  })
  return result.sort((a, b) => a.time - b.time)
}

const findMatchedExtraText = (extras: LyricLine[], time: number): string | undefined => {
  const matched = extras.find((line) => Math.abs(line.time - time) < 0.35)
  return matched?.text
}

const parseLyric = (body: Lyric): LyricLine[] => {
  const mainLines = parseTimedLines(body.lrc?.lyric ?? '')
  const translatedLines = parseTimedLines(body.tlyric?.lyric ?? '')
  const pronunciationLines = parseTimedLines(body.romalrc?.lyric ?? '')

  return mainLines.map((line) => ({
    ...line,
    translation: findMatchedExtraText(translatedLines, line.time),
    pronunciation: findMatchedExtraText(pronunciationLines, line.time)
  }))
}

const fetchLyrics = async (id: number): Promise<void> => {
  const requestId = ++lyricRequestId
  try {
    loading.value = true
    lineRefs.value = []
    const res = (await window.api.lyric({ id })) as { body?: Lyric }
    if (requestId !== lyricRequestId) return
    if (res.body?.lrc?.lyric) {
      lyrics.value = parseLyric(res.body)
    } else {
      lyrics.value = [{ time: 0, text: '暂无歌词' }]
    }
  } catch {
    if (requestId !== lyricRequestId) return
    lyrics.value = [{ time: 0, text: '歌词加载失败' }]
  } finally {
    if (requestId === lyricRequestId) {
      loading.value = false
      scrollActiveLyricToCenter('auto')
    }
  }
}

const handleUserScroll = (): void => {
  isUserScrolling.value = true
  if (userScrollTimer) {
    window.clearTimeout(userScrollTimer)
  }
  userScrollTimer = window.setTimeout(() => {
    isUserScrolling.value = false
  }, 3000)
}

watch(
  () => props.songId,
  (newId) => {
    lyricRequestId++
    lyrics.value = []
    lineRefs.value = []
    scrollContainerRef.value?.scrollTo({ top: 0, behavior: 'auto' })
    if (newId) fetchLyrics(newId)
  },
  { immediate: true }
)

watch(showPronunciation, (value) => {
  localStorage.setItem(LYRIC_PRONUNCIATION_VISIBLE_KEY, String(value))
})

watch(showTranslation, (value) => {
  localStorage.setItem(LYRIC_TRANSLATION_VISIBLE_KEY, String(value))
})

// --- 逻辑优化：提前量计算 ---
const currentLyricIndex = computed(() => {
  // 加上偏移量，让 index 的切换提前发生
  const adjustedTime = (props.currentTime + LYRIC_OFFSET_MS) / 1000
  return lyrics.value.findLastIndex((l) => adjustedTime >= l.time)
})

// --- 滚动逻辑 ---
const scrollActiveLyricToCenter = (behavior: 'auto' | 'smooth' = 'smooth'): void => {
  const activeIndex = currentLyricIndex.value
  if (activeIndex === -1) return

  nextTick(() => {
    const activeEl = lineRefs.value[activeIndex]
    if (activeEl) {
      activeEl.scrollIntoView({
        behavior,
        block: 'center'
      })
    }
  })
}

watch(currentLyricIndex, () => {
  if (isUserScrolling.value) return
  scrollActiveLyricToCenter()
})

watch([showPronunciation, showTranslation], () => {
  scrollActiveLyricToCenter('auto')
})
</script>

<template>
  <section class="lyrics-panel" :style="themeVars">
    <div class="lyric-toolbar">
      <button
        v-if="hasPronunciation"
        type="button"
        class="lyric-toggle"
        :class="{ active: showPronunciation }"
        :title="showPronunciation ? '隐藏发音' : '显示发音'"
        :aria-label="showPronunciation ? '隐藏发音' : '显示发音'"
        @click="showPronunciation = !showPronunciation"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 10v4" />
          <path d="M8 7v10" />
          <path d="M12 4v16" />
          <path d="M16 7v10" />
          <path d="M20 10v4" />
        </svg>
      </button>
      <button
        v-if="hasTranslation"
        type="button"
        class="lyric-toggle"
        :class="{ active: showTranslation }"
        :title="showTranslation ? '隐藏翻译' : '显示翻译'"
        :aria-label="showTranslation ? '隐藏翻译' : '显示翻译'"
        @click="showTranslation = !showTranslation"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 5h9" />
          <path d="M8.5 3v2" />
          <path d="M10.5 5c-.9 3.2-2.8 5.5-5.5 7" />
          <path d="M6 8c1.2 1.9 2.8 3.2 4.8 4" />
          <path d="M14 19l3.5-8 3.5 8" />
          <path d="M15.2 16h4.6" />
        </svg>
      </button>
    </div>
    <div v-if="loading" class="lyric-status">加载中...</div>
    <div
      v-else
      ref="scrollContainerRef"
      class="lyrics-scroll-container"
      @wheel.passive="handleUserScroll"
      @touchstart.passive="handleUserScroll"
    >
      <div
        v-for="(line, index) in lyrics"
        :key="index"
        :ref="
          (el) => {
            if (el) lineRefs[index] = el as HTMLElement
          }
        "
        class="lyric-line"
        :class="{ active: index === currentLyricIndex }"
      >
        <span class="lyric-text">{{ line.text }}</span>
        <span
          v-if="showPronunciation && line.pronunciation"
          class="lyric-extra lyric-pronunciation"
        >
          {{ line.pronunciation }}
        </span>
        <span v-if="showTranslation && line.translation" class="lyric-extra lyric-translation">
          {{ line.translation }}
        </span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.lyrics-panel {
  position: relative;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.lyric-toolbar {
  position: absolute;
  bottom: 14px;
  right: 20px;
  z-index: 2;
  display: flex;
  gap: 8px;
  -webkit-app-region: no-drag;
}

.lyric-toggle {
  width: 32px;
  height: 32px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.12);
  color: var(--lrc-status-color);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  backdrop-filter: blur(18px) saturate(160%);
  transition:
    background 0.2s ease,
    border-color 0.2s ease,
    color 0.2s ease,
    opacity 0.2s ease;
}

.lyric-toggle svg {
  width: 17px;
  height: 17px;
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.lyric-toggle:hover,
.lyric-toggle.active {
  background: rgba(255, 255, 255, 0.22);
  border-color: rgba(255, 255, 255, 0.2);
  color: var(--lrc-text-active-color);
}

.lyrics-scroll-container {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden; /* 严禁左右滚动 */
  padding: 50% 40px; /* 左右 Padding 必须足够大，防止 scale 后的文字被 mask 截断 */
  scroll-behavior: smooth;
  scrollbar-width: none;
  pointer-events: auto;
  /* 增加遮罩的平滑度 */
  mask-image: linear-gradient(to bottom, transparent 0%, black 20%, black 80%, transparent 100%);
  -webkit-mask-image: linear-gradient(
    to bottom,
    transparent 0%,
    black 20%,
    black 80%,
    transparent 100%
  );
  /* 启用硬件加速 */
  will-change: scroll-position;
}
.lyrics-scroll-container::-webkit-scrollbar {
  display: none;
}

.lyric-line {
  /* 使用 margin 而不是 padding 来控制行间距，这样 scale 不会影响行高布局 */
  margin: 12px 0;
  padding: 10px 0;
  line-height: 1.4;
  text-align: center;

  /* 优化过渡曲线：out-expo 风格，开始快，结束慢，视觉上更灵敏 */
  transition:
    transform 0.4s cubic-bezier(0.23, 1, 0.32, 1),
    opacity 0.4s cubic-bezier(0.23, 1, 0.32, 1),
    filter 0.4s ease,
    color 0.3s ease; /* 增加颜色过渡 */

  opacity: 0.3;
  /* 使用 CSS 变量控制颜色 */
  color: var(--lrc-text-color);
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
  /* 使用 CSS 变量控制激活态颜色 */
  color: var(--lrc-text-active-color);
  text-shadow: 0 0 18px var(--lrc-text-shadow);
}

.lyric-text {
  display: inline-block;
  /* 限制宽度防止溢出容器 */
  max-width: 100%;
  word-wrap: break-word;
  white-space: pre-wrap;
}

.lyric-extra {
  display: block;
  max-width: 100%;
  margin-top: 4px;
  color: var(--lrc-status-color);
  font-weight: 500;
  line-height: 1.35;
  word-wrap: break-word;
  white-space: pre-wrap;
}

.lyric-pronunciation {
  font-size: 15px;
  opacity: 0.78;
}

.lyric-translation {
  font-size: 16px;
  opacity: 0.86;
}

.lyric-line.active .lyric-extra {
  color: var(--lrc-text-color);
}

.lyric-status {
  height: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
  font-size: 18px;
  color: var(--lrc-status-color);
}
</style>
