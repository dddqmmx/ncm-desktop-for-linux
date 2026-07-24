import { electronAPI } from '@electron-toolkit/preload'
import { ipcRenderer, webUtils } from 'electron'
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
  captcha_sent: invoke('music:captcha_sent'),
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
  like: invoke('music:like'),
  likelist: invoke('music:likelist'),
  playlist_track_add: invoke('music:playlistTrackAdd'),
  playlist_track_delete: invoke('music:playlistTrackDelete'),
  song_url: invoke('music:songUrl'),
  artist_detail: invoke('music:artistDetail'),
  artist_top_song: invoke('music:artistTopSong'),
  artist_album: invoke('music:artistAlbum'),
  artist_mv: invoke('music:artistMv'),
  album: invoke('music:album'),
  configureXeapi: invoke('music:configureXeapi'),

  // --- Rust 播放器操作 (多参数或特定逻辑) ---
  play_url: invokeArgs('player:playUrl'),
  play_url_cached: invokeArgs('player:playUrlCached'),
  play_file: invokeArgs('player:playFile'),
  get_file_duration: invoke('player:getFileDuration'),
  pause: invoke('player:pause'),
  resume: invoke('player:resume'),
  stop: invoke('player:stop'),
  get_progress: invoke('player:getProgress'),
  is_buffering: invoke('player:isBuffering'),
  seek: invoke('player:seek'),
  switch_output_device: invokeArgs('player:switchOutputDevice'),
  get_output_devices: invoke('player:getOutputDevices'),
  wait_finished: invoke('player:waitFinished'),
  song_url_and_wait: invokeArgs('music:playUrlAndWait'),

  // --- Native Cache API ---
  cache_get_stats: invoke('cache:getStats'),
  cache_set_max_size: invoke('cache:setMaxSizeBytes'),
  cache_get_song_max_cache_ahead_bytes: invoke('cache:getSongMaxCacheAheadBytes'),
  cache_set_song_max_cache_ahead_bytes: invoke('cache:setSongMaxCacheAheadBytes'),
  cache_clear: invoke('cache:clear'),
  resolve_cached_media_url: invoke('cache:resolveCachedMediaUrl'),
  prepare_cached_song_source: invokeArgs('cache:prepareSongSource'),
  cache_song_source: invokeArgs('cache:cacheSongSource'), // { songId, quality, url, expectedBytes?, durationMs? }
  update_song_cache_playback_position: invokeArgs('cache:updateSongPlaybackPosition'),
  cancel_song_cache_download: invoke('cache:cancelSongDownload'),
  get_cached_song_progress: invoke('cache:getSongCacheProgress'),

  //ui相关工具方法
  open_settings_window: invoke('ui:openSettingsWindow'),
  close_settings_window: invoke('ui:closeSettingsWindow'),
  open_dialog_window: invoke('ui:openDialogWindow'),
  get_app_info: invoke('ui:getAppInfo'),

  // 本地曲库
  select_library_folder: (): Promise<unknown> => ipcRenderer.invoke('library:selectFolder'),
  scan_library_folders: (paths: string[]): Promise<unknown> =>
    ipcRenderer.invoke('library:scanFolders', paths),
  check_library_files: (paths: string[]): Promise<unknown> =>
    ipcRenderer.invoke('library:checkFiles', paths),
  get_library_file_durations: (paths: string[]): Promise<unknown> =>
    ipcRenderer.invoke('library:getFileDurations', paths),

  // 将用户在文件输入框中选择的 File 转换为原生路径。
  get_path_for_file: (file: File): string => webUtils.getPathForFile(file)
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
