<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { ref, computed, watch, nextTick } from 'vue'
import type { Lyric } from '@renderer/types/lyric'
import { usePlayerStore } from '@renderer/stores/playerStore'
import { useConfigStore } from '@renderer/stores/configStore'

const props = defineProps<{
  songId?: number
  currentTime: number // 假设是毫秒
  isDark: boolean
  isSeeking?: boolean
}>()

const playerStore = usePlayerStore()
const configStore = useConfigStore()
const lyricDebug = computed(() => configStore.lyricDebug)

// --- 配置常量 ---
const LYRIC_OFFSET_MS = 50 // 仅补偿人眼感知延迟（~50ms），CSS 动画已拆分为快速响应阶段
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
let lastCurrentTime = 0 // 用于检测 seek 跳变

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
  const timeExp = /\[(\d{2}):(\d{2})\.(\d{1,3})\]/
  lines.forEach((line) => {
    const match = timeExp.exec(line)
    if (match) {
      const m = parseInt(match[1])
      const s = parseInt(match[2])
      const msStr = match[3]
      // 使用字符串长度驱动的归一化："4"→0.4, "45"→0.45, "456"→0.456
      const msFraction = parseInt(msStr) / Math.pow(10, msStr.length)
      const time = m * 60 + s + msFraction
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
      nextTick(() => {
        requestAnimationFrame(() => {
          scrollActiveLyricToCenter('auto')
        })
      })
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

// --- 逻辑优化：二分搜索 + 提前量计算 ---
const currentLyricIndex = computed(() => {
  const adjustedTime = (props.currentTime + LYRIC_OFFSET_MS) / 1000
  const lines = lyrics.value
  if (lines.length === 0) return -1
  // 二分搜索：找到最后一个 time <= adjustedTime 的行
  let lo = 0
  let hi = lines.length - 1
  let result = -1
  while (lo <= hi) {
    const mid = (lo + hi) >>> 1
    if (lines[mid].time <= adjustedTime) {
      result = mid
      lo = mid + 1
    } else {
      hi = mid - 1
    }
  }
  return result
})

// [调试] HUD 显示用：当前命中行的索引/时间戳/文本
const debugActiveLine = computed(() => {
  const i = currentLyricIndex.value
  const line = i >= 0 ? lyrics.value[i] : undefined
  return line ? `#${i} @${line.time.toFixed(2)}s ${line.text.slice(0, 18)}` : `#${i}`
})

// --- 滚动逻辑 ---
const scrollActiveLyricToCenter = (behavior: 'auto' | 'smooth' = 'smooth'): void => {
  const activeIndex = currentLyricIndex.value
  if (activeIndex === -1) return

  if (lyricDebug.value) {
    console.log(
      `[LYRIC] scrollActiveLyricToCenter: index=${activeIndex}, behavior=${behavior}, time=${(props.currentTime / 1000).toFixed(2)}s`
    )
  }
  nextTick(() => {
    const activeEl = lineRefs.value[activeIndex]
    const scrollContainer = scrollContainerRef.value
    if (!activeEl || !scrollContainer) {
      if (lyricDebug.value)
        console.log(
          `[lyric] scroll skipped idx=${activeIndex} el=${!!activeEl} cont=${!!scrollContainer}`
        )
      return
    }

    if (behavior === 'auto') {
      scrollContainer.scrollTop =
        activeEl.offsetTop - scrollContainer.clientHeight / 2 + activeEl.clientHeight / 2
    } else {
      activeEl.scrollIntoView({
        behavior,
        block: 'center'
      })
    }
    if (lyricDebug.value)
      console.log(
        `[lyric] scroll(${behavior}) idx=${activeIndex} offsetTop=${activeEl.offsetTop} scrollTop=${Math.round(scrollContainer.scrollTop)}`
      )
  })
}

watch(currentLyricIndex, (newIndex, oldIndex) => {
  if (isUserScrolling.value) return
  if (lyricDebug.value) {
    console.log(
      `[LYRIC] index changed: ${oldIndex} -> ${newIndex}, time=${(props.currentTime / 1000).toFixed(2)}s, isSeeking=${props.isSeeking}`
    )
  }
  scrollActiveLyricToCenter()
})

// --- Seek 跳变检测：当 currentTime 发生大幅跳变时，立即定位歌词 ---
watch(
  () => props.currentTime,
  (newTime) => {
    const delta = Math.abs(newTime - lastCurrentTime)
    lastCurrentTime = newTime
    // 超过 1 秒的跳变视为 seek 操作，立即定位
    if (delta > 1000) {
      if (lyricDebug.value) {
        console.log(
          `[LYRIC] SEEK JUMP detected: delta=${delta.toFixed(0)}ms, newTime=${newTime.toFixed(0)}ms`
        )
      }
      // 取消用户滚动锁定
      isUserScrolling.value = false
      if (userScrollTimer) {
        window.clearTimeout(userScrollTimer)
        userScrollTimer = undefined
      }
      scrollActiveLyricToCenter('auto')
    }
  }
)

// --- isSeeking prop 变化：seek 结束后立即同步歌词位置 ---
watch(
  () => props.isSeeking,
  (seeking, wasSeeking) => {
    if (lyricDebug.value) {
      console.log(`[LYRIC] isSeeking prop: ${wasSeeking} -> ${seeking}`)
    }
    if (wasSeeking && !seeking) {
      if (lyricDebug.value) {
        console.log(`[LYRIC] Seek ended, forcing scroll to current lyric`)
      }
      isUserScrolling.value = false
      scrollActiveLyricToCenter('auto')
    }
  }
)

watch([showPronunciation, showTranslation], () => {
  scrollActiveLyricToCenter('auto')
})
</script>

<template>
  <section class="lyrics-panel" :style="themeVars">
    <div v-if="lyricDebug" class="lyric-debug-hud">
      <div>clock(currentTime): {{ Math.round(props.currentTime) }} ms</div>
      <div>
        backend raw: {{ playerStore.rawProgressMs }} ms (Δ
        {{ Math.round(props.currentTime - playerStore.rawProgressMs) }})
      </div>
      <div>
        play={{ playerStore.isPlaying }} buf={{ playerStore.isBuffering }} load={{
          playerStore.isLoading
        }}
        userScroll={{ isUserScrolling }}
      </div>
      <div>lyric: {{ debugActiveLine }}</div>
    </div>
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
        <AppIcon name="equalizer" :size="18" />
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
        <AppIcon name="translate" :size="18" />
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
        :class="{
          active: index === currentLyricIndex,
          'near-active': Math.abs(index - currentLyricIndex) <= 15
        }"
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

.lyric-debug-hud {
  position: absolute;
  top: 8px;
  left: 8px;
  z-index: 10;
  padding: 6px 10px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.6);
  color: #0f0;
  font-family: monospace;
  font-size: 12px;
  line-height: 1.5;
  pointer-events: none;
  white-space: nowrap;
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
  /* 注意：不设置 scroll-behavior: smooth，滚动行为完全由 JS scrollIntoView 控制 */
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

.lyrics-panel.snapping .lyrics-scroll-container {
  scroll-behavior: auto;
}

.lyrics-scroll-container::-webkit-scrollbar {
  display: none;
}

.lyric-line {
  margin: 12px 0;
  padding: 10px 0;
  line-height: 1.4;
  text-align: center;

  /* 基础过渡，不包含 GPU 密集型属性 */
  transition:
    opacity 0.15s cubic-bezier(0.23, 1, 0.32, 1),
    color 0.15s ease;

  opacity: 0.3;
  color: var(--lrc-text-color);
  font-size: 26px;
  font-weight: 500;

  transform-origin: center center;

  /* CSS 虚拟渲染核心：让浏览器跳过不可见元素的布局和绘制 */
  content-visibility: auto;
  contain-intrinsic-size: 0 56px; /* 估算的基础高度 (26px * 1.4 + margin/padding) */
}

/* 仅对即将在屏幕上出现的歌词应用 GPU 硬件加速和模糊特效，避免上百个图层撑爆 GPU */
.lyric-line.near-active {
  /* 关键：防止抖动的核心属性，并移入视窗内的元素 */
  will-change: transform, opacity;
  backface-visibility: hidden;
  filter: blur(0.5px); /* 未激活时轻微模糊，增加层次感 */

  /* 恢复平滑的完整过渡 */
  transition:
    transform 0.35s cubic-bezier(0.23, 1, 0.32, 1),
    opacity 0.15s cubic-bezier(0.23, 1, 0.32, 1),
    filter 0.35s ease,
    color 0.15s ease;
}

.lyric-line.active {
  opacity: 1;
  font-weight: 700;
  transform: scale(1.1);
  color: var(--lrc-text-active-color);
  text-shadow: 0 0 18px var(--lrc-text-shadow);

  /* 仅激活行提升到独立的 GPU 合成层 */
  will-change: transform, opacity;
  backface-visibility: hidden;
  transform-origin: center center;
}

.lyrics-panel.snapping .lyric-line,
.lyrics-panel.snapping .lyric-line.active {
  transition: none;
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
