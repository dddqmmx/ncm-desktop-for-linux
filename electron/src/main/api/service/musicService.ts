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
  song_url as song_url_legacy,
  song_url_v1,
  song_download_url_v1,
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
  captcha_sent,
  like,
  likelist,
  comment_music,
  playlist_tracks,
  playlist_create,
  playlist_delete,
  batch
} from '@neteasecloudmusicapienhanced/api'
import { createRequire } from 'node:module'
import { CacheService } from './cacheService'
import {
  normalizePlaylistTracksError,
  normalizeTrackIds,
  unwrapPlaylistTracksResponse,
  type PlaylistTrackMutationResult,
  type PlaylistTrackOp
} from './playlistTracks'
import {
  normalizePlaylistUpdateResult,
  playlistUpdateRequest,
  type PlaylistUpdateParams
} from './playlistMetadata'

type ServiceResult<T = APIBaseResponse> = {
  status: number
  body: T | null
  cookie?: string[]
  error?: string
}

type CacheBucket = 'song' | 'entity' | 'lyric'
type SongUrlParams = Parameters<typeof song_url_v1>[0]
type LegacySongUrlParams = Parameters<typeof song_url_legacy>[0]
type SongUrlItem = Record<string, unknown> & {
  br?: number
  level?: string
}
type SongUrlBody = APIBaseResponse & {
  data?: SongUrlItem[]
}
const nodeRequire = createRequire(import.meta.url)
const generateConfig = nodeRequire(
  '@neteasecloudmusicapienhanced/api/generateConfig'
) as () => Promise<void>

function configureXeapi(): Promise<void> {
  return generateConfig().catch((error) => {
    console.warn('[musicService] 更新网易云 xeapi 公钥失败:', error)
  })
}

void configureXeapi()

function bitrateForLegacySongUrl(level: SongUrlParams['level']): number {
  if (level === 'standard') return 128000
  if (level === 'exhigh') return 320000
  return 999000
}

function inferLegacyLevel(item: SongUrlItem): string {
  if (typeof item.level === 'string' && item.level) return item.level
  if (typeof item.br === 'number' && item.br >= 999000) return 'lossless'
  if (typeof item.br === 'number' && item.br >= 320000) return 'exhigh'
  return 'standard'
}

async function fetchLegacySongUrl(params: SongUrlParams): Promise<Response<SongUrlBody>> {
  const legacyParams: LegacySongUrlParams = {
    id: params.id,
    br: bitrateForLegacySongUrl(params.level),
    cookie: params.cookie
  }
  const res = (await song_url_legacy(legacyParams)) as Response<SongUrlBody>
  const data = Array.isArray(res.body?.data)
    ? res.body.data.map((item) => ({
        ...item,
        level: inferLegacyLevel(item)
      }))
    : res.body?.data

  return {
    ...res,
    body: res.body
      ? {
          ...res.body,
          data
        }
      : res.body
  }
}

async function fetchSongUrl(params: SongUrlParams): Promise<Response<SongUrlBody>> {
  try {
    return (await song_url_v1(params)) as Response<SongUrlBody>
  } catch (error) {
    if (!(error instanceof Error && error.message.includes('xeapi public key'))) throw error
  }

  await configureXeapi()

  try {
    return (await song_url_v1(params)) as Response<SongUrlBody>
  } catch (error) {
    if (!(error instanceof Error && error.message.includes('xeapi public key'))) throw error
  }

  console.warn(
    '[musicService] xeapi public key is missing; falling back to legacy song_url endpoint'
  )
  return fetchLegacySongUrl(params)
}

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
    .catch((e: unknown) => ({
      status: 500,
      body: null,
      error: describeRequestFailure(e)
    }))
}

/**
 * 请求失败时库抛出的是 `{ status, body }`（业务错误）或 axios 错误，
 * 只有 Error 实例才带 message；直接读 e.message 会把真实原因丢成 undefined。
 */
function describeRequestFailure(error: unknown): string {
  if (error instanceof Error) return error.message

  const rejected = error as { body?: { message?: unknown; msg?: unknown } } | null
  for (const candidate of [rejected?.body?.message, rejected?.body?.msg]) {
    if (typeof candidate === 'string' && candidate.trim()) return candidate.trim()
  }

  return typeof error === 'string' ? error : '请求失败'
}

// 线性退避步长：失败后立即重试只会在网络抖动或服务端限流时雪上加霜
const RETRY_BACKOFF_STEP_MS = 250

const retry = <T>(fn: () => Promise<T>, maxRetries = 5): Promise<T> => {
  let attempt = 0

  const run = (): Promise<T> => {
    return fn().catch((err) => {
      if (attempt >= maxRetries) throw err
      attempt++
      return new Promise<T>((resolve) => {
        setTimeout(() => resolve(run()), RETRY_BACKOFF_STEP_MS * attempt)
      })
    })
  }

  return run()
}

const createMethod = <P, T>(fn: (params: P) => Promise<Response<T>>) => {
  return (params: P) => responseHandler(() => fn(params))
}

/**
 * 歌单曲目增删。
 *
 * 不能用库里的 playlist_track_add / playlist_track_delete：
 * - add 走的是已下线的 `/api/playlist/track/add`，服务端一律回 401「无权限操作歌单」，
 *   哪怕歌单是自己建的、曲目已经在里面（与曲目本身、cookie 是否有效都无关）；
 * - delete 读的是 `query.id`，而调用方传的是 `pid`，歌单 id 根本传不进去。
 *
 * 两者都改走 `/api/playlist/manipulate/tracks`（库里的 playlist_tracks），
 * 也就是网页版增删/拖动曲目用的接口。
 */
const mutatePlaylistTracks = (op: PlaylistTrackOp) => {
  return async (params: {
    pid: number | string
    ids: string | number | (string | number)[]
    cookie?: string
  }): Promise<PlaylistTrackMutationResult> => {
    const trackIds = normalizeTrackIds(params.ids)
    if (trackIds.length === 0) {
      return { status: 400, body: null, error: '缺少要操作的歌曲' }
    }

    try {
      return unwrapPlaylistTracksResponse(
        await playlist_tracks({
          ...(params.cookie ? { cookie: params.cookie } : {}),
          pid: Number(params.pid),
          tracks: trackIds.join(','),
          op
        })
      )
    } catch (error) {
      return normalizePlaylistTracksError(error)
    }
  }
}

function cacheKey(scope: string, params: Record<string, unknown>): string {
  return CacheService.buildKey({ scope, ...params })
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
  configureXeapi,
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
    (params: { cookie?: string }) => cacheKey('user_account', { cookie: params.cookie ?? '' }),
    createMethod(user_account)
  ),
  song_url: createMethod(fetchSongUrl),
  song_download_url: createMethod(song_download_url_v1),
  // 下载权限会随账号、会员和购买状态变化，不能复用按歌曲 ID 缓存的详情。
  song_download_detail: createMethod(song_detail),
  playlist_catlist: createMethod(playlist_catlist),
  user_playlist: createMethod(user_playlist),
  playlist_detail: createMethod(playlist_detail),
  lyric: createCachedMethod(
    'lyric',
    (params: { id: number | string }) => cacheKey('lyric', { id: params.id }),
    createMethod(lyric_new)
  ),
  recommend_resource: createMethod(recommend_resource),
  recommend_songs: createMethod(recommend_songs),
  like: createMethod(like),
  likelist: createMethod(likelist),
  comment_music: createMethod(comment_music),
  playlist_track_add: mutatePlaylistTracks('add'),
  playlist_track_delete: mutatePlaylistTracks('del'),
  playlist_create: createMethod(playlist_create),
  playlist_update: async (params: PlaylistUpdateParams) =>
    normalizePlaylistUpdateResult(
      await responseHandler(() => batch(playlistUpdateRequest(params)))
    ),
  playlist_delete: createMethod(playlist_delete),
  artist_detail: createCachedMethod(
    'entity',
    (params: { id: number | string }) => cacheKey('artist_detail', { id: params.id }),
    createMethod(artist_detail)
  ),
  artist_top_song: createCachedMethod(
    'entity',
    (params: { id: number | string }) => cacheKey('artist_top_song', { id: params.id }),
    createMethod(artist_top_song)
  ),
  artist_album: createCachedMethod(
    'entity',
    (params: { id: number | string; limit?: number; offset?: number }) =>
      cacheKey('artist_album', {
        id: params.id,
        limit: params.limit ?? 0,
        offset: params.offset ?? 0
      }),
    createMethod(artist_album)
  ),
  artist_mv: createCachedMethod(
    'entity',
    (params: { id: number | string; limit?: number; offset?: number }) =>
      cacheKey('artist_mv', {
        id: params.id,
        limit: params.limit ?? 0,
        offset: params.offset ?? 0
      }),
    createMethod(artist_mv)
  ),
  album: createCachedMethod(
    'entity',
    (params: { id: number | string }) => cacheKey('album', { id: params.id }),
    createMethod(album)
  ),
  song_detail(params: { ids: number[] | string; [key: string]: unknown }) {
    const ids = Array.isArray(params.ids) ? params.ids.join(',') : params.ids
    return createCachedMethod(
      'song',
      (normalizedParams: typeof params) =>
        cacheKey('song_detail', {
          ids: Array.isArray(normalizedParams.ids)
            ? normalizedParams.ids.join(',')
            : normalizedParams.ids
        }),
      (normalizedParams: typeof params) =>
        responseHandler(() => song_detail({ ...normalizedParams, ids }))
    )({ ...params, ids })
  }
}
