<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { resolveCachedMediaUrl } from '@renderer/utils/cache'

/**
 * 真正意义上的懒加载图片组件
 *
 * 特点：
 * 1. 使用 IntersectionObserver 监测可见性，只有进入视口时才开始解析和加载。
 * 2. 封装了 resolveCachedMediaUrl 逻辑，优先从本地缓存获取。
 * 3. 组件不控制尺寸，由父布局控制（width: 100%, height: 100%）。
 */
interface Props {
  /**
   * 图片的唯一标识（通常是图片的原始 URL）
   * 在本项目中，URL 是缓存系统的 Key，因此它充当了 ID 的角色
   */
  id: string | number | null | undefined
  /** 图片描述 */
  alt?: string
  /**
   * 网易云图片参数，如 '200y200', '400y400'
   * 会被自动拼接到 URL 末尾：?param=...
   */
  param?: string
  /** 图片类型，可用于显示不同的占位图效果 */
  type?: 'avatar' | 'cover' | 'playlist' | 'artist'
  /** 是否需要圆角（如头像场景），默认为 false */
  rounded?: boolean
}

const props = defineProps<Props>()

const containerRef = ref<HTMLElement | null>(null)
const isVisible = ref(false) // 是否已进入视口范围
const resolvedUrl = ref('') // 解析后的最终 URL (缓存 file:// 或原始 http://)
const isLoaded = ref(false) // 真正图片资源是否已加载完成
const hasError = ref(false) // 是否加载失败

let observer: IntersectionObserver | null = null

/**
 * 获取带参数的原始 URL
 */
const getRawUrlWithParam = (): string => {
  if (!props.id) return ''
  let url = String(props.id).trim()
  if (!url) return ''

  // 如果是 HTTP 地址且提供了参数，则拼接参数
  if (url.startsWith('http') && props.param && !url.includes('?param=')) {
    // 简单拼接，假设原 URL 不带其他复杂参数或已处理好
    url = `${url}?param=${props.param}`
  }
  return url
}

/**
 * 执行解析并触发图片加载
 */
const resolveAndLoad = async (): Promise<void> => {
  // 如果已解析过或者当前不可见，则跳过
  if (resolvedUrl.value || !isVisible.value) return

  const sourceUrl = getRawUrlWithParam()
  if (!sourceUrl) return

  try {
    // 只有在真正需要显示时，才通过 IPC 向主进程解析缓存路径
    // 这一步是性能优化的核心：避免非视口内图片的无效 IPC 开销
    const url = await resolveCachedMediaUrl(sourceUrl)
    resolvedUrl.value = url || sourceUrl
  } catch (e) {
    console.warn('[LazyImage] 解析缓存失败，回退至原始地址:', e)
    resolvedUrl.value = sourceUrl
  }
}

/**
 * 处理交叉观察回调
 */
const handleIntersect = (entries: IntersectionObserverEntry[]): void => {
  const [entry] = entries
  if (entry.isIntersecting) {
    isVisible.value = true
    resolveAndLoad()

    // 一旦触发加载，通常可以停止观察以节省系统资源
    if (observer && containerRef.value) {
      observer.unobserve(containerRef.value)
    }
  }
}

onMounted(() => {
  // 初始化观察器
  observer = new IntersectionObserver(handleIntersect, {
    // 提前 100px 开始加载，提供更平滑的滚动体验
    rootMargin: '100px',
    threshold: 0.01
  })

  if (containerRef.value) {
    observer.observe(containerRef.value)
  }
})

onUnmounted(() => {
  if (observer) {
    observer.disconnect()
  }
})

// 监听 ID 或参数变化，支持列表刷新或组件复用
watch(
  () => [props.id, props.param],
  () => {
    // 重置内部状态
    resolvedUrl.value = ''
    isLoaded.value = false
    hasError.value = false

    // 如果当前已经是可见状态，立即重新解析
    if (isVisible.value) {
      resolveAndLoad()
    }
  },
  { deep: false }
)

const onImgLoad = (): void => {
  isLoaded.value = true
}

const onImgError = (): void => {
  hasError.value = true
  isLoaded.value = true // 标记加载尝试结束
}
</script>

<template>
  <div ref="containerRef" class="lazy-image-wrapper" :class="[type, { 'is-rounded': rounded }]">
    <!-- 1. 实际显示的图片层 -->
    <transition name="lazy-img-fade">
      <img
        v-if="resolvedUrl"
        :src="resolvedUrl"
        :alt="alt"
        class="lazy-img-element"
        :class="{ 'is-ready': isLoaded }"
        @load="onImgLoad"
        @error="onImgError"
      />
    </transition>

    <!-- 2. 加载中/占位层 -->
    <div v-if="!isLoaded && !hasError" class="lazy-image-placeholder">
      <slot name="placeholder">
        <!-- 这里可以放一个默认的 SVG 占位符或背景渐变 -->
        <div class="placeholder-content"></div>
      </slot>
    </div>

    <!-- 3. 错误状态层 -->
    <div v-if="hasError" class="lazy-image-error">
      <slot name="error">
        <span class="error-msg">!</span>
      </slot>
    </div>
  </div>
</template>

<style scoped>
.lazy-image-wrapper {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background-color: rgba(0, 0, 0, 0.04);
  display: flex;
  align-items: center;
  justify-content: center;
  /* 确保父容器的 borderRadius 能够限制住子元素 */
  border-radius: inherit;
}

.is-rounded {
  border-radius: 50% !important;
}

.lazy-img-element {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  opacity: 0;
  will-change: opacity;
}

/* 图片解码并加载完成后的渐显效果 */
.lazy-img-element.is-ready {
  opacity: 1;
}

.lazy-image-placeholder {
  position: absolute;
  inset: 0;
  z-index: 1;
  background: linear-gradient(135deg, rgba(235, 237, 240, 0.8), rgba(220, 225, 235, 0.6));
  display: flex;
  align-items: center;
  justify-content: center;
}

.placeholder-content {
  width: 32px;
  height: 32px;
  opacity: 0.15;
  background: currentColor;
  mask: url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M21 19V5c0-1.1-.9-2-2-2H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2zM8.5 13.5l2.5 3.01L14.5 12l4.5 6H5l3.5-4.5z"/></svg>')
    no-repeat center;
  -webkit-mask: url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M21 19V5c0-1.1-.9-2-2-2H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2zM8.5 13.5l2.5 3.01L14.5 12l4.5 6H5l3.5-4.5z"/></svg>')
    no-repeat center;
}

.lazy-image-error {
  position: absolute;
  inset: 0;
  z-index: 2;
  background-color: #f0f1f3;
  color: #c0c4cc;
  display: flex;
  align-items: center;
  justify-content: center;
}

.error-msg {
  font-size: 20px;
  font-weight: bold;
}

/* 渐显过渡动画 */
.lazy-img-fade-enter-active {
  transition: opacity 0.4s ease-out;
}

.lazy-img-fade-enter-from {
  opacity: 0;
}
</style>
