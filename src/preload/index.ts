import { electronAPI } from '@electron-toolkit/preload'
import { ipcRenderer } from 'electron'
import { contextBridge } from 'electron/renderer'

/**
 * 优化点：工厂函数
 * 预先绑定 IPC 调度逻辑，减少重复代码解析开销
 */
const invoke =
  (channel: string) =>
  (params?: unknown): Promise<unknown> =>
    ipcRenderer.invoke(channel, params)
const invokeArgs =
  (channel: string) =>
  (...args: unknown[]): Promise<unknown> =>
    ipcRenderer.invoke(channel, ...args)

const api = {
  // --- 网易云 API (单参数模式) ---
  login: invoke('music:login'),
  banner: invoke('music:banner'),
  userCloud: invoke('music:userCloud'),
  search: invoke('music:search'),
  song_detail: invoke('music:songDetail'),
  login_qr_key: invoke('music:loginQrKey'),
  login_qr_create: invoke('music:loginQrCreate'),
  login_qr_check: invoke('music:loginQrCheck'),
  user_account: invoke('music:userAccount'),
  playlist_catlist: invoke('music:playlistCatlist'),
  user_playlist: invoke('music:userPlaylist'),
  playlist_detail: invoke('music:playlistDetail'),
  lyric: invoke('music:lyric'),
  recommend_resource: invoke('music:recommendResource'),
  recommend_songs: invoke('music:recommendSongs'),
  song_url: invoke('music:songUrl'),

  // --- Rust 播放器操作 (多参数或特定逻辑) ---
  // 使用 invokeArgs 支持多参数透传，避免手动展开
  play_url: invokeArgs('player:playUrl'),
  play_file: invokeArgs('player:playFile'),
  pause: invoke('player:pause'),
  resume: invoke('player:resume'),
  stop: invoke('player:stop'),
  get_progress: invoke('player:getProgress'),
  seek: invoke('player:seek'),
  wait_finished: invoke('player:waitFinished'),
  song_url_and_wait: invokeArgs('music:playUrlAndWait'),
}

// 保持不变
if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('electron', electronAPI)
    contextBridge.exposeInMainWorld('api', api)
  } catch (error) {
    console.error(error)
  }
} else {
  // @ts-ignore -- defined in preload d.ts for non-isolated context
  window.electron = electronAPI
  // @ts-ignore -- defined in preload d.ts for non-isolated context
  window.api = api
}
