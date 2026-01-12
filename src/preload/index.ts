import { contextBridge, ipcRenderer } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'

// Custom APIs for renderer
const api = {
  // 网易云api操作
  login: (p: unknown) => ipcRenderer.invoke('music:login', p),
  banner: (p: unknown) => ipcRenderer.invoke('music:banner', p),
  userCloud: (p: unknown) => ipcRenderer.invoke('music:userCloud', p),
  search: (p: unknown) => ipcRenderer.invoke('music:search', p),
  song_detail: (p: unknown) => ipcRenderer.invoke('music:songDetail', p),
  login_qr_key: (p: unknown) => ipcRenderer.invoke('music:loginQrKey', p),
  login_qr_create: (p: unknown) => ipcRenderer.invoke('music:loginQrCreate', p),
  login_qr_check: (p: unknown) => ipcRenderer.invoke('music:loginQrCheck', p),
  user_account: (p: unknown) => ipcRenderer.invoke('music:userAccount', p),
  playlist_catlist: (p:unknown)  => ipcRenderer.invoke('music:playlistCatlist', p),
  user_playlist: (p:unknown)  => ipcRenderer.invoke('music:userPlaylist', p),
  playlist_detail: (p:unknown)  => ipcRenderer.invoke('music:playlistDetail', p),
  lyric: (p:unknown)  => ipcRenderer.invoke('music:lyric', p),
  recommend_resource: (p:unknown)  => ipcRenderer.invoke('music:recommendResource', p),
  recommend_songs: (p:unknown) => ipcRenderer.invoke("music:recommendSongs", p),
  // rust后端播放器操作
  song_url: (p: unknown) => ipcRenderer.invoke('music:songUrl', p),
  play_url: (url:string, startSecs?: number) => ipcRenderer.invoke('player:playUrl', url,startSecs),
  play_file: (filePath:string, startSecs?: number) => ipcRenderer.invoke('player:playFile', filePath,startSecs),
  pause: () => ipcRenderer.invoke('player:pause'),
  resume: () => ipcRenderer.invoke('player:resume'),
  stop: () => ipcRenderer.invoke('player:stop'),
  get_progress: () => ipcRenderer.invoke('player:getProgress'),
  seek: (time:number)  => ipcRenderer.invoke('player:seek', time),
  wait_finished: () => ipcRenderer.invoke('player:waitFinished'),
  song_url_and_wait: (url:string, startSecs?: number) => ipcRenderer.invoke('music:playUrlAndWait', url, startSecs),
}

if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('electron', electronAPI)
    contextBridge.exposeInMainWorld('api', api)
  } catch (error) {
    console.error(error)
  }
} else {
  // @ts-ignore (define in dts)
  window.electron = electronAPI
  // @ts-ignore (define in dts)
  window.api = api
}
