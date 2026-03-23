import { getNativeModule } from '../native/loadNativeModule'

const { PlayerService } = getNativeModule()

const player = new PlayerService()

export const NativeService = {
  // 发送指令，立刻返回
  playUrl(url: string, startSecs?: number) {
    try {
      // 这里的 player.playUrl 是 Rust NAPI 导出的同步方法（只发消息）
      return player.playUrl(url, startSecs)
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
    startSecs?: number
  ) {
    try {
      return player.playUrlCached(
        url,
        cachePath,
        metadataPath,
        durationMs,
        cacheAheadSecs,
        startSecs
      )
    } catch (e) {
      console.error('Native playUrlCached Error:', e)
      throw e
    }
  },

  playFile(filePath: string, start_secs?: number) {
    return player.playFile(filePath, start_secs)
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

  /**
   * 获取播放状态
   */
  isPlaying() {
    return player.isPlaying
  },

  async seek(time: number) {
    return player.seek(time)
  },

  async switchOutputDevice(deviceId?: string) {
    return player.switchOutputDevice(deviceId)
  },

  async getOutputDevices() {
    return player.getOutputDevices()
  },

  /**
   * 只有这个方法真正需要 await
   */
  async waitFinished() {
    try {
      return await player.waitFinished()
    } catch (e) {
      console.warn('waitFinished interrupted or player stopped:', e)
    }
  },

  /**
   * 组合逻辑：
   * 适用于：播放完这首，自动切下一首
   */
  async playUrlAndWait(url: string, startSecs?: number) {
    this.playUrl(url, startSecs)
    await this.waitFinished()
  }
}
