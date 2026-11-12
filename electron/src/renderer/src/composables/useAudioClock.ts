/**
 * 提供一个与物理音频时钟对齐的单调时间源。
 *
 * 优先使用 Web Audio API（AudioContext.currentTime / getOutputTimestamp），
 * 它已经扣除了音频设备的输出延迟，能更准确地反映“当前正在从扬声器播出的是哪一帧”。
 * 在 AudioContext 不可用的环境中，退而使用一个隐藏的 <audio> 元素作为时间源；
 * 再不可用则使用 performance.now() 作为最后兜底。
 */
export interface AudioClock {
  /**
   * 当前音频输出时间（秒）。
   * 基于音频硬件时钟，近似等于“用户实际听到的声音”的播放位置。
   */
  currentTimeSeconds: number

  /** 当前估计的端到端音频设备延迟（秒）。 */
  outputLatencySeconds: number

  /** 确保时钟正在运行（例如用户手势后恢复 AudioContext）。 */
  resume(): Promise<void>
}

const writeString = (view: DataView, offset: number, value: string): void => {
  for (let i = 0; i < value.length; i++) {
    view.setUint8(offset + i, value.charCodeAt(i))
  }
}

/**
 * 生成一段极短的静音 WAV，供 <audio> 回退方案作为可播放源，
 * 使其 currentTime 能够真正随时间推进。
 */
const createSilentWavBlob = (durationSec = 0.25, sampleRate = 8000): Blob => {
  const numSamples = Math.floor(sampleRate * durationSec)
  const headerSize = 44
  const buffer = new ArrayBuffer(headerSize + numSamples)
  const view = new DataView(buffer)

  writeString(view, 0, 'RIFF')
  view.setUint32(4, 36 + numSamples, true)
  writeString(view, 8, 'WAVE')
  writeString(view, 12, 'fmt ')
  view.setUint32(16, 16, true)
  view.setUint16(20, 1, true) // PCM
  view.setUint16(22, 1, true) // mono
  view.setUint32(24, sampleRate, true)
  view.setUint32(28, sampleRate, true)
  view.setUint16(32, 1, true)
  view.setUint16(34, 8, true)
  writeString(view, 36, 'data')
  view.setUint32(40, numSamples, true)

  const bytes = new Uint8Array(buffer)
  bytes.fill(128, headerSize)

  return new Blob([bytes], { type: 'audio/wav' })
}

/**
 * 使用 performance.now() 的兜底时钟。
 * 精度不如 AudioContext，但能在 Node / 测试环境中工作。
 */
const createPerformanceClock = (): AudioClock => ({
  get currentTimeSeconds() {
    return performance.now() / 1000
  },
  get outputLatencySeconds() {
    return 0
  },
  resume: async () => {}
})

const createAudioElementClock = (): AudioClock => {
  const audio = document.createElement('audio')
  audio.loop = true
  audio.muted = true
  audio.preload = 'auto'

  return {
    get currentTimeSeconds() {
      return audio.currentTime
    },
    get outputLatencySeconds() {
      return 0
    },
    resume: async () => {
      if (audio.src) {
        void audio.play()
        return
      }
      try {
        audio.src = URL.createObjectURL(createSilentWavBlob())
        await audio.play()
      } catch {
        // 如果静音回退也无法播放，currentTime 将停留在 0；
        // 上层仍可通过与后端的同步锚点保持基本可用。
      }
    }
  }
}

const createAudioContextClock = (): AudioClock => {
  const AudioContextCtor: typeof AudioContext | undefined =
    (window as unknown as { AudioContext?: typeof AudioContext }).AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext

  if (!AudioContextCtor) {
    throw new Error('AudioContext not supported')
  }

  const ctx = new AudioContextCtor({ latencyHint: 'playback' })

  // 连接一个增益为 0 的节点，让 AudioContext 被浏览器视为“正在输出音频”，
  // 降低在后台或节能场景下被意外挂起的概率，同时不会产生可闻声音。
  try {
    const gain = ctx.createGain()
    gain.gain.value = 0
    gain.connect(ctx.destination)

    const osc = ctx.createOscillator()
    osc.frequency.value = 1
    osc.connect(gain)
    osc.start()
  } catch {
    // 即使无法创建节点，currentTime 仍会在 running 状态下推进。
  }

  return {
    get currentTimeSeconds() {
      try {
        // getOutputTimestamp 返回的是当前正从设备输出的采样帧时间，
        // 已经反映了输出延迟，是最贴近“实际出声时刻”的值。
        const ts = ctx.getOutputTimestamp()
        const contextTime = ts.contextTime
        if (contextTime !== undefined && Number.isFinite(contextTime) && contextTime > 0) {
          return Math.max(0, contextTime)
        }
      } catch {
        // 某些环境不支持 getOutputTimestamp，退到 currentTime。
      }
      const latency = (ctx.outputLatency ?? 0) + (ctx.baseLatency ?? 0)
      return Math.max(0, ctx.currentTime - latency)
    },
    get outputLatencySeconds() {
      return (ctx.outputLatency ?? 0) + (ctx.baseLatency ?? 0)
    },
    resume: async () => {
      if (ctx.state === 'suspended') {
        await ctx.resume()
      }
    }
  }
}

let cachedClock: AudioClock | null = null

export function useAudioClock(): AudioClock {
  if (cachedClock) return cachedClock

  if (typeof window === 'undefined') {
    cachedClock = createPerformanceClock()
    return cachedClock
  }

  const w = window as unknown as {
    AudioContext?: typeof AudioContext
    webkitAudioContext?: typeof AudioContext
  }

  if (typeof AudioContext !== 'undefined' || typeof w.webkitAudioContext !== 'undefined') {
    try {
      cachedClock = createAudioContextClock()
      return cachedClock
    } catch {
      // fallthrough
    }
  }

  if (
    typeof HTMLAudioElement !== 'undefined' &&
    typeof Blob !== 'undefined' &&
    typeof URL !== 'undefined'
  ) {
    cachedClock = createAudioElementClock()
    return cachedClock
  }

  cachedClock = createPerformanceClock()
  return cachedClock
}
