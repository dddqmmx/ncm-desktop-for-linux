import { getNativeModule } from '../native/loadNativeModule'
import type { NativeFinishReason } from '../native/loadNativeModule'

const { PlayerService } = getNativeModule()

const player = new PlayerService()

export const NativeService = {
  playUrl(url: string, startSecs?: number, strictBitPerfect?: boolean, exclusive?: boolean) {
    try {
      return player.playUrl(url, startSecs, strictBitPerfect, exclusive)
    } catch (e) {
      console.error('Native playUrl Error:', e)
      throw e
    }
  },

  playUrlCached(
    url: string,
    cachePath: string,
    metadataPath: string,
    durationMs?: number,
    cacheAheadSecs?: number,
    maxCacheAheadBytes?: number,
    startSecs?: number,
    strictBitPerfect?: boolean,
    exclusive?: boolean
  ) {
    try {
      return player.playUrlCached(
        url,
        cachePath,
        metadataPath,
        durationMs,
        cacheAheadSecs,
        maxCacheAheadBytes,
        startSecs,
        strictBitPerfect,
        exclusive
      )
    } catch (e) {
      console.error('Native playUrlCached Error:', e)
      throw e
    }
  },

  playFile(filePath: string, start_secs?: number, strictBitPerfect?: boolean, exclusive?: boolean) {
    return player.playFile(filePath, start_secs, strictBitPerfect, exclusive)
  },

  getFileDuration(filePath: string) {
    return player.getFileDurationMs(filePath)
  },

  pause() {
    return player.pause()
  },

  resume() {
    return player.resume()
  },

  stop() {
    return player.stop()
  },

  getProgress() {
    return player.progressMs
  },

  isBuffering() {
    return player.isBuffering
  },

  /**
   * 播放进度 + 缓冲状态 + 会话音量/锁定一次取回：渲染层每 200ms 轮询一次，
   * 合并成单次 IPC 往返可以把稳态轮询开销减半。会话音量是 SetVolume 的
   * 回读权威——渲染层据此校对滑块与实际生效增益是否失步。
   */
  getPlaybackStatus() {
    return {
      progressMs: player.progressMs,
      isBuffering: player.isBuffering,
      volume: player.volume,
      volumeLocked: player.volumeLocked
    }
  },

  /**
   * 获取播放状态
   */
  isPlaying() {
    return player.isPlaying
  },

  async seek(time: number) {
    return player.seek(time)
  },

  /**
   * 应用级软件音量（0.0~1.0），仅影响本应用输出，不改系统音量。
   */
  setVolume(volume: number) {
    return player.setVolume(volume)
  },

  async switchOutputDevice(deviceId?: string, strictBitPerfect?: boolean, exclusive?: boolean) {
    return player.switchOutputDevice(deviceId, strictBitPerfect, exclusive)
  },

  async getOutputDevices() {
    return player.getOutputDevices()
  },

  /**
   * 阻塞到当前播放会话结束，返回结束原因：
   * 'ended' 自然播完 / 'stopped' 显式停止 / 'replaced' 被新播放替换 / 'error' 流致命错误。
   * 通道被中断（worker 退出等）时返回 undefined，调用方按“无需处理”对待。
   */
  async waitFinished(): Promise<NativeFinishReason | undefined> {
    try {
      return await player.waitFinished()
    } catch (e) {
      console.warn('waitFinished interrupted or player stopped:', e)
      return undefined
    }
  }
}
