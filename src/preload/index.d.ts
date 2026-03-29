import { ElectronAPI } from '@electron-toolkit/preload'

interface CustomApi {
  // 网易云api操作
  login: (p: unknown) => Promise<unknown>
  banner: (p: unknown) => Promise<unknown>
  userCloud: (p: unknown) => Promise<unknown>
  search: (p: unknown) => Promise<unknown>
  song_detail: (p: unknown) => Promise<unknown>
  login_qr_key: (p: unknown) => Promise<unknown>
  login_qr_create: (p: unknown) => Promise<unknown>
  login_qr_check: (p: unknown) => Promise<unknown>
  user_account: (p: unknown) => Promise<unknown>
  song_url: (p: unknown) => Promise<unknown>
  playlist_catlist: (p: unknown) => Promise<unknown>
  user_playlist: (p: unknown) => Promise<unknown>
  playlist_detail: (p: unknown) => Promise<unknown>
  lyric: (p: unknown) => Promise<unknown>
  recommend_resource: (p: unknown) => Promise<unknown>
  recommend_songs: (p: unknown) => Promise<unknown>
  artist_detail: (p: unknown) => Promise<unknown>
  artist_top_song: (p: unknown) => Promise<unknown>
  artist_album: (p: unknown) => Promise<unknown>
  artist_mv: (p: unknown) => Promise<unknown>
  album: (p: unknown) => Promise<unknown>
  // rust后端播放器操作
  play_url: (url: string, startSecs?: number) => Promise<unknown>
  play_url_cached: (
    url: string,
    cachePath: string,
    metadataPath: string,
    durationMs?: number,
    cacheAheadSecs?: number,
    startSecs?: number
  ) => Promise<unknown>
  play_file: (filePath: string, startSecs?: number) => Promise<unknown>
  pause: () => Promise<unknown>
  resume: () => Promise<unknown>
  stop: () => Promise<unknown>
  get_progress: () => Promise<number>
  seek: (time: number) => Promise<unknown>
  switch_output_device: (deviceId?: string) => Promise<unknown>
  get_output_devices: () => Promise<AudioDeviceInfo[]>
  wait_finished: () => Promise<unknown>
  song_url_and_wait: (url: string, startSecs?: number) => Promise<unknown>
  cache_get_stats: () => Promise<CacheStats>
  cache_set_max_size: (maxSizeBytes: number) => Promise<CacheStats>
  cache_get_song_cache_ahead_secs: () => Promise<number>
  cache_set_song_cache_ahead_secs: (songCacheAheadSecs: number) => Promise<number>
  cache_clear: () => Promise<CacheStats>
  resolve_cached_media_url: (url: string) => Promise<string>
  prepare_cached_song_source: (payload: {
    songId: number
    quality: string
    url: string
  }) => Promise<CachedSongSource>
  //ui相关工具方法
  open_settings_window: () => Promise<unknown>
  close_settings_window: () => Promise<unknown>
  get_app_info: () => Promise<AppInfo>
}

interface AppInfo {
  name: string
  version: string
}

interface AudioDeviceInfo {
  id: string
  name: string
  isDefault: boolean
  isCurrent: boolean
}

interface CacheStats {
  totalBytes: number
  maxSizeBytes: number
  songBytes: number
  songEntries: number
  entityBytes: number
  entityEntries: number
  coverBytes: number
  coverEntries: number
  lyricBytes: number
  lyricEntries: number
}

interface CachedSongSource {
  type: 'file' | 'url'
  value: string
  cachePath?: string
  metadataPath?: string
  cacheAheadSecs?: number
}

declare global {
  interface Window {
    electron: ElectronAPI
    api: CustomApi
  }
}
