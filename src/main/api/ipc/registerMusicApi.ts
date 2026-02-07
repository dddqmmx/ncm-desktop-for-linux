import { ipcMain } from 'electron'
import { MusicService } from '../service/musicService'

type MusicApiHandler = (params: unknown) => Promise<unknown>


export function registerMusicApi(): void {
  // 基础的一对一映射
  const simpleMappings = {
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
  } as Record<string, MusicApiHandler>

  // 批量注册
  Object.entries(simpleMappings).forEach(([channel, method]) => {
    ipcMain.handle(channel, (_event, params) => method(params))
  })

  // 2. 特殊逻辑单独处理 (参数结构转换)
  ipcMain.handle('music:banner', (_event, type) => MusicService.getBanner({ type }))

  console.log('Netease Music API registered successfully.')
}
