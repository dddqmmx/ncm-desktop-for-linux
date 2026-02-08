import path from "path";
import fs from "fs"; // 需要引入 fs 模块来做路径探测
import { app } from "electron";

function resolveNative(): string {
  // 1. dev（electron-vite dev）
  if (!app.isPackaged) {
    return path.join(__dirname, '..', '..', 'native', 'index.node')
  }

  const appPath = app.getAppPath()

  // 2. system electron / AUR（无 asar）
  // appPath 形如：/usr/lib/xxx/out 或 /home/.../out
  if (!appPath.endsWith('.asar')) {
    return path.join(appPath, 'native', 'index.node')
  }

  // 3. asar 安装（AppImage / deb）
  const asarDir = path.dirname(appPath)
  const asarNative = path.join(asarDir, 'native', 'index.node')
  if (fs.existsSync(asarNative)) {
    return asarNative
  }

  // 4. 最后兜底（AppImage）
  return path.join(process.resourcesPath, 'native', 'index.node')
}


// eslint-disable-next-line @typescript-eslint/no-require-imports -- native .node must be loaded via require
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
