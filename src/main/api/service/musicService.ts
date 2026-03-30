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
  cloudsearch,
  user_account,
  song_url_v1,
  playlist_catlist,
  user_playlist,
  playlist_detail,
  lyric_new,
  recommend_resource,
  recommend_songs,
  artist_detail,
  artist_top_song,
  artist_album,
  artist_mv,
  album,
  captcha_sent
} from 'NeteaseCloudMusicApi'
import { CacheService } from './cacheService'

type ServiceResult<T = APIBaseResponse> = {
  status: number
  body: T | null
  cookie?: string[]
  error?: string
}

type CacheBucket = 'song' | 'entity' | 'lyric'

/**
 * 优化后的响应处理器
 * 1. 使用 .then/.catch 减少 async 状态机开销
 * 2. 移除冗余的 await
 */
const responseHandler = <T = APIBaseResponse>(
  apiCall: () => Promise<Response<T>>
): Promise<ServiceResult<T>> => {
  return retry(apiCall, 5)
    .then((res) => ({
      status: res.status,
      body: res.body,
      cookie: res.cookie
    }))
    .catch((e: Error) => ({
      status: 500,
      body: null,
      error: e.message
    }))
}

const retry = <T>(fn: () => Promise<T>, maxRetries = 5): Promise<T> => {
  let attempt = 0

  const run = (): Promise<T> => {
    return fn().catch((err) => {
      if (attempt >= maxRetries) throw err
      attempt++
      return run()
    })
  }

  return run()
}

const createMethod = <P, T>(fn: (params: P) => Promise<Response<T>>) => {
  return (params: P) => responseHandler(() => fn(params))
}

function isCacheableSuccess<T>(result: ServiceResult<T>): result is ServiceResult<T> & { body: T } {
  return result.status >= 200 && result.status < 300 && result.body !== null
}

const createCachedMethod = <P, T>(
  bucket: CacheBucket,
  keyBuilder: (params: P) => string,
  fn: (params: P) => Promise<ServiceResult<T>>
) => {
  return async (params: P): Promise<ServiceResult<T>> => {
    const cacheKey = keyBuilder(params)
    const cachedBody = await CacheService.getJson<T>(bucket, cacheKey)
    if (cachedBody !== null) {
      return {
        status: 200,
        body: cachedBody
      }
    }

    const result = await fn(params)
    if (isCacheableSuccess(result)) {
      await CacheService.setJson(bucket, cacheKey, result.body)
    }

    return result
  }
}

export const MusicService = {
  login: createMethod(login_cellphone),
  captcha_sent: createMethod(captcha_sent),
  getBanner: createMethod(banner),
  getUserCloud: createMethod(user_cloud),
  search(params: Parameters<typeof cloudsearch>[0]) {
    return createMethod(cloudsearch)(params).then((res) => {
      // 兼容旧接口的字段结构
      const body = res.body as Record<string, unknown>
      const result = body?.result as Record<string, unknown>
      if (result?.songs && Array.isArray(result.songs)) {
        result.songs = result.songs.map((songItem) => {
          const song = songItem as Record<string, unknown>
          return {
            ...song,
            artists: song.ar || song.artists,
            album: song.al || song.album,
            duration: song.dt || song.duration
          }
        })
      }
      return res
    })
  },
  login_qr_key: createMethod(login_qr_key),
  login_qr_create: createMethod(login_qr_create),
  login_qr_check: createMethod(login_qr_check),
  user_account: createCachedMethod(
    'entity',
    (params: { cookie?: string }) =>
      CacheService.buildKey({
        scope: 'user_account',
        cookie: params.cookie ?? ''
      }),
    createMethod(user_account)
  ),
  song_url: createMethod(song_url_v1),
  playlist_catlist: createMethod(playlist_catlist),
  user_playlist: createCachedMethod(
    'entity',
    (params: { uid: number }) =>
      CacheService.buildKey({
        scope: 'user_playlist',
        uid: params.uid
      }),
    createMethod(user_playlist)
  ),
  playlist_detail: createMethod(playlist_detail),
  lyric: createCachedMethod(
    'lyric',
    (params: { id: number | string }) =>
      CacheService.buildKey({
        scope: 'lyric',
        id: params.id
      }),
    createMethod(lyric_new)
  ),
  recommend_resource: createMethod(recommend_resource),
  recommend_songs: createMethod(recommend_songs),
  artist_detail: createCachedMethod(
    'entity',
    (params: { id: number | string }) =>
      CacheService.buildKey({
        scope: 'artist_detail',
        id: params.id
      }),
    createMethod(artist_detail)
  ),
  artist_top_song: createCachedMethod(
    'entity',
    (params: { id: number | string }) =>
      CacheService.buildKey({
        scope: 'artist_top_song',
        id: params.id
      }),
    createMethod(artist_top_song)
  ),
  artist_album: createCachedMethod(
    'entity',
    (params: { id: number | string; limit?: number; offset?: number }) =>
      CacheService.buildKey({
        scope: 'artist_album',
        id: params.id,
        limit: params.limit ?? 0,
        offset: params.offset ?? 0
      }),
    createMethod(artist_album)
  ),
  artist_mv: createCachedMethod(
    'entity',
    (params: { id: number | string; limit?: number; offset?: number }) =>
      CacheService.buildKey({
        scope: 'artist_mv',
        id: params.id,
        limit: params.limit ?? 0,
        offset: params.offset ?? 0
      }),
    createMethod(artist_mv)
  ),
  album: createCachedMethod(
    'entity',
    (params: { id: number | string }) =>
      CacheService.buildKey({
        scope: 'album',
        id: params.id
      }),
    createMethod(album)
  ),
  song_detail(params: { ids: number[] | string; [key: string]: unknown }) {
    const ids = Array.isArray(params.ids) ? params.ids.join(',') : params.ids
    return createCachedMethod(
      'song',
      (normalizedParams: typeof params) =>
        CacheService.buildKey({
          scope: 'song_detail',
          ids: Array.isArray(normalizedParams.ids)
            ? normalizedParams.ids.join(',')
            : normalizedParams.ids
        }),
      (normalizedParams: typeof params) =>
        responseHandler(() => song_detail({ ...normalizedParams, ids }))
    )({ ...params, ids })
  }
}
