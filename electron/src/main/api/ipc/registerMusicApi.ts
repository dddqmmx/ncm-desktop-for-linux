import { BrowserWindow, ipcMain } from 'electron'
import { MusicService } from '../service/musicService'
import { DownloadService } from '../service/downloadService'
import type { SongDownloadRequest, SongDownloadOptionsRequest } from '../../../shared/download'

type MusicApiHandler = (params: unknown) => Promise<unknown>

export function registerMusicApi(): void {
  // 基础的一对一映射
  const simpleMappings = {
    'music:login': MusicService.login,
    'music:captcha_sent': MusicService.captcha_sent,
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
    'music:like': MusicService.like,
    'music:likelist': MusicService.likelist,
    'music:commentMusic': MusicService.comment_music,
    'music:playlistTrackAdd': MusicService.playlist_track_add,
    'music:playlistTrackDelete': MusicService.playlist_track_delete,
    'music:playlistCreate': MusicService.playlist_create,
    'music:playlistUpdate': MusicService.playlist_update,
    'music:playlistDelete': MusicService.playlist_delete,
    'music:songUrl': MusicService.song_url,
    'music:artistDetail': MusicService.artist_detail,
    'music:artistTopSong': MusicService.artist_top_song,
    'music:artistAlbum': MusicService.artist_album,
    'music:artistMv': MusicService.artist_mv,
    'music:album': MusicService.album,
    'music:configureXeapi': MusicService.configureXeapi as MusicApiHandler
  } as Record<string, MusicApiHandler>

  // 批量注册
  Object.entries(simpleMappings).forEach(([channel, method]) => {
    ipcMain.handle(channel, (_event, params) => method(params))
  })

  // 2. 特殊逻辑单独处理 (参数结构转换)
  ipcMain.handle('music:banner', (_event, type) => MusicService.getBanner({ type }))

  ipcMain.handle('music:downloadSong', (event, payload: SongDownloadRequest) =>
    DownloadService.downloadSong(payload, BrowserWindow.fromWebContents(event.sender))
  )
  ipcMain.handle('music:downloadSongQualities', (_event, payload: SongDownloadOptionsRequest) =>
    DownloadService.getQualityOptions(payload)
  )

  console.log('Netease Music API registered successfully.')
}
