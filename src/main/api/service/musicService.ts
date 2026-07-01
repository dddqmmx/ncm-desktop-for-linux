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
  likelist
} from '@neteasecloudmusicapienhanced/api'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { resolve } from 'node:path'
import { CacheService } from './cacheService'

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
type XeapiPublicKey = Record<string, unknown> & {
  sk?: unknown
}
type XeapiKeyModule = {
  getXeapiPublicKey: (
    currentPublicKey?: XeapiPublicKey,
    deviceId?: string
  ) => Promise<XeapiPublicKey>
}

const nodeRequire = createRequire(import.meta.url)
const { getXeapiPublicKey } = nodeRequire(
  '@neteasecloudmusicapienhanced/api/util/xeapiKey'
) as XeapiKeyModule
const xeapiPublicKeyPath = resolve(tmpdir(), 'xeapi_public_key')
const XEAPI_PUBLIC_KEY_MISSING_ERROR = 'xeapi public key is missing'
let xeapiConfigPromise: Promise<void> | null = null

function readXeapiPublicKey(): XeapiPublicKey {
  if (!existsSync(xeapiPublicKeyPath)) return {}

  try {
    return JSON.parse(readFileSync(xeapiPublicKeyPath, 'utf-8')) as XeapiPublicKey
  } catch {
    return {}
  }
}

function hasXeapiPublicKey(): boolean {
  const publicKey = readXeapiPublicKey()
  return typeof publicKey.sk === 'string' && publicKey.sk.length > 0
}

function isXeapiPublicKeyMissing(error: unknown): boolean {
  return error instanceof Error && error.message.includes(XEAPI_PUBLIC_KEY_MISSING_ERROR)
}

function getGlobalDeviceId(): string {
  const deviceId = (globalThis as { deviceId?: unknown }).deviceId
  return typeof deviceId === 'string' ? deviceId : ''
}

async function refreshXeapiPublicKey(): Promise<void> {
  const publicKey = await getXeapiPublicKey(readXeapiPublicKey(), getGlobalDeviceId())
  writeFileSync(xeapiPublicKeyPath, JSON.stringify(publicKey), 'utf-8')
}

function startXeapiConfigGeneration(): Promise<void> {
  if (!xeapiConfigPromise) {
    xeapiConfigPromise = Promise.resolve()
      .then(() => refreshXeapiPublicKey())
      .catch((error) => {
        console.warn('[musicService] 更新网易云 xeapi 公钥失败:', error)
      })
      .finally(() => {
        xeapiConfigPromise = null
      })
  }
  return xeapiConfigPromise
}

async function ensureXeapiConfig(force = false): Promise<void> {
  if (!force && hasXeapiPublicKey()) return
  await startXeapiConfigGeneration()
}

void ensureXeapiConfig()

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
  await ensureXeapiConfig()

  if (hasXeapiPublicKey()) {
    try {
      return (await song_url_v1(params)) as Response<SongUrlBody>
    } catch (error) {
      if (!isXeapiPublicKeyMissing(error)) {
        throw error
      }

      await ensureXeapiConfig(true)
      if (hasXeapiPublicKey()) {
        return (await song_url_v1(params)) as Response<SongUrlBody>
      }
    }
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
