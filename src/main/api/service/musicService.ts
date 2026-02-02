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

/**
 * 优化后的响应处理器
 * 1. 使用 .then/.catch 减少 async 状态机开销
 * 2. 移除冗余的 await
 */
const responseHandler = <T = APIBaseResponse>(
  apiCall: Promise<Response<T>>,
): Promise<ServiceResult<T>> => {
  return apiCall
    .then((res) => ({
      status: res.status,
      body: res.body,
      cookie: res.cookie,
    }))
    .catch((e: Error) => ({
      status: 500,
      body: null,
      error: e.message,
    }));
};


const createMethod = <P, T>(fn: (params: P) => Promise<Response<T>>) => {
  return (params: P) => responseHandler(fn(params));
};

export const MusicService = {
  login: createMethod(login_cellphone),
  getBanner: createMethod(banner),
  getUserCloud: createMethod(user_cloud),
  search: createMethod(search),
  login_qr_key: createMethod(login_qr_key),
  login_qr_create: createMethod(login_qr_create),
  login_qr_check: createMethod(login_qr_check),
  user_account: createMethod(user_account),
  song_url: createMethod(song_url_v1),
  playlist_catlist: createMethod(playlist_catlist),
  user_playlist: createMethod(user_playlist),
  playlist_detail: createMethod(playlist_detail),
  lyric: createMethod(lyric_new),
  recommend_resource: createMethod(recommend_resource),
  recommend_songs: createMethod(recommend_songs),

  song_detail(params: { ids: number[] | string | any }) {
    const ids = Array.isArray(params.ids) ? params.ids.join(',') : params.ids;
    return responseHandler(song_detail({ ...params, ids }));
  },
};
