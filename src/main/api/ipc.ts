import { ipcMain } from 'electron'
import { MusicService, NativeService } from './service'
import { SoundQualityType } from 'NeteaseCloudMusicApi'


// 1. 定义一个通用的转发辅助函数，减少闭包逻辑
const handle = (channel: string, serviceMethod: (params: any) => Promise<any>) => {
  // 直接传递函数，不使用 async/await 包装，减少微任务层级
  ipcMain.handle(channel, (_, params) => serviceMethod(params));
};

export function registerMusicApi(): void {
  // 基础的一对一映射
  const simpleMappings: Record<string, (params: any) => Promise<any>> = {
    'music:login': MusicService.login,
    'music:userCloud': MusicService.getUserCloud,
    'music:search': MusicService.search,
    'music:songDetail': MusicService.song_detail,
    'music:loginQrKey': MusicService.login_qr_key,
    'music:loginQrCreate': MusicService.login_qr_create,
    'music:loginQrCheck': MusicService.login_qr_check,
    'music:userAccount': MusicService.user_account,
    'music:playlistCatlist': MusicService.playlist_catlist,
    'music:userPlaylist': MusicService.user_playlist,
    'music:playlistDetail': MusicService.playlist_detail,
    'music:lyric': MusicService.lyric,
    'music:recommendResource': MusicService.recommend_resource,
    'music:recommendSongs': MusicService.recommend_songs,
    'music:songUrl': MusicService.song_url, // 如果参数结构一致，也可以放这里
  };

  // 批量注册
  Object.entries(simpleMappings).forEach(([channel, method]) => {
    ipcMain.handle(channel, (_, params) => method(params));
  });

  // 2. 特殊逻辑单独处理 (参数结构转换)
  ipcMain.handle('music:banner', (_, type) => MusicService.getBanner({ type }));

  console.log('Netease Music API registered successfully.');
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
