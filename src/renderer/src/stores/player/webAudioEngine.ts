/**
 * 浏览器原生音频引擎（WebAPI engine）。
 *
 * 作为 native(Rust/cpal) 播放链路的对照实现：用一个 HTMLAudioElement 播放歌曲。
 * 与 native 路径共用同一套缓存准备逻辑，优先播放已缓存的本地文件，未命中缓存时
 * 回退到网络 URL，由浏览器自行拉取；缓存工具会按浏览器的实际播放进度增量缓存。
 *
 * 播放位置以 `audio.currentTime` 为准——这是浏览器“当前实际正在输出的位置”，
 * 由浏览器自己扣除输出延迟。用它来对照排查：歌词/进度与真实声音的同步问题
 * 究竟出在前端(歌词逻辑)还是后端(native 音频管线)。
 */

import type { CachedSongSource } from '@renderer/types/cache'

type VoidCb = () => void

interface WebEngineCallbacks {
  onEnded?: VoidCb
  onBuffering?: (buffering: boolean) => void
  onPlayStateChange?: (playing: boolean) => void
  onError?: (message: string) => void
}

let audioEl: HTMLAudioElement | null = null
let callbacks: WebEngineCallbacks = {}
let pendingStartSec = 0

/** 将本地缓存绝对路径转成项目已注册的 ncm-cache:// 协议 URL。 */
function cacheFileToUrl(path: string): string {
  return `ncm-cache://asset?path=${encodeURIComponent(path)}`
}

export function toAudioUrl(source: CachedSongSource): string {
  const value = source.value
  if (!value) return value

  // 命中本地文件缓存时，使用主进程自定义协议访问，避免 Electron 拦截 file:// 资源
  if (source.type === 'file') {
    return cacheFileToUrl(value)
  }

  if (
    value.startsWith('http:') ||
    value.startsWith('https:') ||
    value.startsWith('ncm-cache:') ||
    value.startsWith('data:') ||
    value.startsWith('blob:')
  ) {
    return value
  }

  // 兜底：如果后端返回了裸路径但 type 不是 file，也尝试走缓存协议
  return cacheFileToUrl(value)
}

function ensureEl(): HTMLAudioElement {
  if (audioEl) return audioEl
  const a = new Audio()
  a.preload = 'auto'
  // 注意：不要设置 crossOrigin —— 纯 <audio> 播放跨域 URL 无需 CORS，
  // 一旦设为 anonymous 反而要求 CDN 返回 CORS 头，网易 CDN 不返回会导致加载失败。

  a.addEventListener('loadedmetadata', () => {
    if (pendingStartSec > 0) {
      try {
        a.currentTime = pendingStartSec
      } catch {
        // 部分流在 metadata 阶段还不可定位，忽略，等 canplay 再试
      }
      pendingStartSec = 0
    }
  })
  a.addEventListener('ended', () => callbacks.onEnded?.())
  a.addEventListener('waiting', () => callbacks.onBuffering?.(true))
  a.addEventListener('stalled', () => callbacks.onBuffering?.(true))
  a.addEventListener('playing', () => callbacks.onBuffering?.(false))
  a.addEventListener('canplay', () => callbacks.onBuffering?.(false))
  a.addEventListener('play', () => callbacks.onPlayStateChange?.(true))
  a.addEventListener('pause', () => callbacks.onPlayStateChange?.(false))
  a.addEventListener('error', () => {
    const code = a.error?.code
    callbacks.onError?.(`WebAPI 播放失败 (code=${code ?? 'unknown'})`)
  })

  audioEl = a
  return a
}

export const webAudioEngine = {
  setCallbacks(cbs: WebEngineCallbacks): void {
    callbacks = cbs
  },

  /**
   * 载入并从 startSec 开始播放一个已准备的缓存源。
   * @param source 缓存准备结果：可能是已缓存的本地文件，也可能是网络 URL
   */
  async load(source: CachedSongSource, startSec = 0): Promise<void> {
    const audioUrl = toAudioUrl(source)
    console.log(
      `[webAudioEngine] load source type=${source.type}, original=${source.value}, resolved=${audioUrl}, cachePath=${source.cachePath ?? '<none>'}, metadataPath=${source.metadataPath ?? '<none>'}`
    )
    const a = ensureEl()
    pendingStartSec = startSec > 0 ? startSec : 0
    a.src = audioUrl
    a.load()
    await a.play()
  },

  async resume(): Promise<void> {
    await ensureEl().play()
  },

  pause(): void {
    if (audioEl) audioEl.pause()
  },

  seek(sec: number): void {
    const a = ensureEl()
    const target = Math.max(0, sec)
    if (a.readyState >= 1) {
      try {
        a.currentTime = target
      } catch {
        pendingStartSec = target
      }
    } else {
      pendingStartSec = target
    }
  },

  stop(): void {
    if (!audioEl) return
    audioEl.pause()
    audioEl.removeAttribute('src')
    audioEl.load()
    pendingStartSec = 0
  },

  get currentTimeMs(): number {
    return audioEl ? audioEl.currentTime * 1000 : 0
  },

  get durationMs(): number {
    const d = audioEl?.duration ?? 0
    return Number.isFinite(d) ? d * 1000 : 0
  },

  get paused(): boolean {
    return audioEl ? audioEl.paused : true
  }
}
