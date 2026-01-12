import { ipcMain } from 'electron'
import { MusicService, NativeService } from './service'
import { SoundQualityType } from 'NeteaseCloudMusicApi'


export function registerMusicApi(): void {
  ipcMain.handle('music:login', async (_, params) => {
    return await MusicService.login(params)
  })

  ipcMain.handle('music:banner', async (_, type) => {
    return await MusicService.getBanner({ type })
  })

  ipcMain.handle('music:userCloud', async (_, params) => {
    return await MusicService.getUserCloud(params)
  })

  ipcMain.handle('music:search', async (_, params) => {
    return await MusicService.search(params)
  })

  ipcMain.handle('music:songDetail', async (_, params) => {
    return await MusicService.song_detail(params)
  })

  ipcMain.handle('music:loginQrKey', async (_, params) => {
    return await MusicService.login_qr_key(params)
  })

  ipcMain.handle('music:loginQrCreate', async (_, params) => {
    return await MusicService.login_qr_create(params)
  })

  ipcMain.handle('music:loginQrCheck', async (_, params) => {
    return await MusicService.login_qr_check(params)
  })

  ipcMain.handle('music:userAccount', async (_, params) => {
      return await MusicService.user_account(params)
  })
  ipcMain.handle(
    'music:songUrl',
    async (
      _,
      params: {
        id: string | number
        level: SoundQualityType
      },
    ) => {
      return await MusicService.song_url(params)
    },
  )
  ipcMain.handle('music:playlistCatlist', async (_, params) => {
      return await MusicService.playlist_catlist(params)
  })
  ipcMain.handle('music:userPlaylist', async (_, params) => {
      return await MusicService.user_playlist(params)
  })
  ipcMain.handle("music:playlistDetail", async (_, params)=>{
    return await MusicService.playlist_detail(params)
  })
  ipcMain.handle("music:lyric", async (_, params)=>{
    return await MusicService.lyric(params)
  })
  ipcMain.handle("music:recommendResource", async (_, params)=>{
    return await MusicService.recommend_resource(params)
  })
  ipcMain.handle("music:recommendSongs", async (_, params)=>{
    return await MusicService.recommend_songs(params)
  })
  console.log('Netease Music API registered successfully.')
}
export function registerNativeApi(): void {
  ipcMain.handle('player:playUrl',  (_, url:string, startSecs?: number) => {
    return NativeService.playUrl(url, startSecs)
  })
  ipcMain.handle('player:playFile',  (_, filePath:string, startSecs?: number) => {
    return NativeService.playFile(filePath, startSecs)
  })
  ipcMain.handle('player:pause',  (_) => {
    return NativeService.pause()
  })
  ipcMain.handle('player:resume',  (_) => {
    return NativeService.resume()
  })
  ipcMain.handle('player:stop',  (_) => {
    return NativeService.stop()
  })
  ipcMain.handle('player:getProgress',  (_) => {
    return NativeService.getProgress()
  })
  ipcMain.handle('player:seek',  (_,time) => {
    return NativeService.seek(time)
  })
  ipcMain.handle('player:waitFinished', () => {
    return NativeService.waitFinished()
  })
  ipcMain.handle('player:playUrlAndWait', (_e, url: string, startSecs?: number) => {
    return NativeService.playUrlAndWait(url, startSecs)
  })
  console.log('Native API registered successfully.')
}
