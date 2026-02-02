import path from "path";
import { app } from "electron";

function resolveNative() {
  // 打包态（AppImage / linux-unpacked 都对）
  if (app.isPackaged) {
    return path.join(
      process.resourcesPath,
      "native",
      "index.node"
    );
  }

  // 开发态
  return path.join(
    __dirname,
    "..",
    "..",
    "native",
    "index.node"
  );
}

const native = require(resolveNative());



const { PlayerService } = native


const player = new PlayerService();

export const NativeService = {
  // 发送指令，立刻返回
  playUrl(url: string, startSecs?: number) {
    try {
      // 这里的 player.playUrl 是 Rust NAPI 导出的同步方法（只发消息）
      return player.playUrl(url, startSecs);
    } catch (e) {
      console.error("Native playUrl Error:", e);
      throw e;
    }
  },

  playFile(filePath: string, start_secs?: number) {
    return player.playFile(filePath, start_secs);
  },

  pause() {
    return player.pause();
  },

  resume() {
    return player.resume();
  },

  stop() {
    return player.stop();
  },

  getProgress() {
    return player.progressMs;
  },

  /**
   * 获取播放状态
   */
  isPlaying() {
    return player.isPlaying;
  },

  async seek(time: number) {
    return player.seek(time);
  },

  /**
   * 只有这个方法真正需要 await
   */
  async waitFinished() {
    try {
      return await player.waitFinished();
    } catch (e) {
      console.warn("waitFinished interrupted or player stopped:", e);
    }
  },

  /**
   * 组合逻辑：
   * 适用于：播放完这首，自动切下一首
   */
  async playUrlAndWait(url: string, startSecs?: number) {
    this.playUrl(url, startSecs);
    await this.waitFinished();
  }
};
