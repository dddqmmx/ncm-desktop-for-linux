import path from 'path'

const native = require(
  import.meta.env.DEV
    ? path.join(__dirname, '../../native/index.node')
    : path.join(process.resourcesPath, 'index.node')
)

const { PlayerService } = native

import {
  type Response,
  type APIBaseResponse,
  login_cellphone,
  user_cloud,
  banner,
  song_detail,
  login_qr_key,
  login_qr_create,
  login_qr_check,
  search,
  user_account,
  song_url_v1,
  playlist_catlist,
  user_playlist,
  playlist_detail,
  lyric_new,
  recommend_resource,
  recommend_songs
} from 'NeteaseCloudMusicApi'

type ServiceResult<T = APIBaseResponse> = {
  status: number
  body: T | null
  cookie?: string[]
  error?: string
}

const responseHandler = async <T = APIBaseResponse>(
  apiCall: Promise<Response<T>>,
): Promise<ServiceResult<T>> => {
  try {
    const res = await apiCall
    return {
      status: res.status,
      body: res.body,
      cookie: res.cookie
    }
  } catch (e) {
    const err = e as Error
    return {
      status: 500,
      body: null,
      error: err.message
    }
  }
}

export const MusicService = {
  login(params: Parameters<typeof login_cellphone>[0]) {
    return responseHandler(login_cellphone(params))
  },

  getBanner(params: Parameters<typeof banner>[0]) {
    return responseHandler(banner(params))
  },

  getUserCloud(params: Parameters<typeof user_cloud>[0]) {
    return responseHandler(user_cloud(params))
  },

  search(params: Parameters<typeof search>[0]) {
    return responseHandler(search(params))
  },

  song_detail(params: { ids: number[] | string }) {
    const idsStr = Array.isArray(params.ids) ? params.ids.join(',') : params.ids
    return responseHandler(song_detail({ ...params, ids: idsStr }))
  },

  login_qr_key(params: Parameters<typeof login_qr_key>[0]) {
    return responseHandler(login_qr_key(params))
  },

  login_qr_create(params: Parameters<typeof login_qr_create>[0]) {
    return responseHandler(login_qr_create(params))
  },

  login_qr_check(params: Parameters<typeof login_qr_check>[0]) {
    return responseHandler(login_qr_check(params))
  },

  user_account(params: Parameters<typeof user_account>[0]) {
    return responseHandler(user_account(params))
  },

  song_url(params: Parameters<typeof song_url_v1>[0]){
    return responseHandler(song_url_v1(params))
  },

  playlist_catlist(params: Parameters<typeof playlist_catlist>[0]){
    return responseHandler(playlist_catlist(params))
  },

  user_playlist(params: Parameters<typeof user_playlist>[0]){
    return responseHandler(user_playlist(params))
  },
  playlist_detail(params: Parameters<typeof playlist_detail>[0]){
    return responseHandler(playlist_detail(params))
  },
  lyric(params: Parameters<typeof lyric_new>[0]){
    return responseHandler(lyric_new(params))
  },
  recommend_resource(params: Parameters<typeof recommend_resource>[0]){
    return responseHandler(recommend_resource(params))
  },
  recommend_songs(params: Parameters<typeof recommend_songs>[0]){
    return responseHandler(recommend_songs(params))
  }
}

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
