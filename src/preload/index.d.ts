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
  song_url: (p:unknown) => Promise<unknown>
  playlist_catlist: (p:unknown) => Promise<unknown>
  user_playlist: (p:unknown) => Promise<unknown>
  playlist_detail: (p:unknown) => Promise<unknown>
  lyric: (p:unknown) => Promise<unknown>
  recommend_resource: (p:unknown) => Promise<unknown>
  recommend_songs: (p:unknown) => Promise<unknown>
  // rust后端播放器操作
  play_url: (url:string, startSecs?: number) => Promise<unknown>
  play_file: (filePath:string, startSecs?: number) => Promise<unknown>
  pause: () => Promise<unknown>
  resume: () => Promise<unknown>
  stop: () => Promise<unknown>
  get_progress: () => Promise<number>
  seek: (time:number) => Promise<unknown>
  wait_finished: () => Promise<unknown>
  song_url_and_wait: (url:string, startSecs?: number) => Promise<unknown>
}

declare global {
  interface Window {
    electron: ElectronAPI
    api: CustomApi
  }
}
