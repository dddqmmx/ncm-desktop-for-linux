/**
 * 歌单曲目增删的纯逻辑部分（不依赖 electron / 网络，便于单测）。
 *
 * 背景：库里自带的 playlist_track_add / playlist_track_delete 在当前服务端已不可用
 * —— add 走 `/api/playlist/track/add`，一律回 401「无权限操作歌单」；
 * delete 误读 `query.id` 而不是 `pid`。两者都改走
 * `/api/playlist/manipulate/tracks`（网页版拖动曲目用的接口）。
 */

export type PlaylistTrackOp = 'add' | 'del'

export interface PlaylistTrackMutationResult {
  status: number
  body: { code?: number; message?: unknown; msg?: unknown } | null
  cookie?: string[]
  error?: string
}

/** 把 "[1,2]" / "1,2" / [1,2] 归一成接口要求的 "1,2" */
export function normalizeTrackIds(ids: string | number | (string | number)[]): string[] {
  if (Array.isArray(ids)) {
    return ids.map((id) => String(id).trim()).filter(Boolean)
  }

  return String(ids)
    .replace(/[[\]\s]/g, '')
    .split(',')
    .map((id) => id.trim())
    .filter(Boolean)
}

/**
 * playlist_tracks 的返回有两处畸形，必须在这里抹平：
 * 1. 业务失败不抛异常，真实结果只在 body.code 里（502 = 歌单内歌曲重复）；
 * 2. add 分支/502-重试分支把整个 response 又包了一层，业务体在 body.body。
 */
export function unwrapPlaylistTracksResponse(raw: unknown): PlaylistTrackMutationResult {
  const res = (raw ?? {}) as {
    status?: number
    body?: { body?: unknown; cookie?: string[] } & Record<string, unknown>
    cookie?: string[]
  }

  const nested = res.body?.body
  const payload = nested && typeof nested === 'object' ? nested : res.body

  return {
    status: res.status ?? 200,
    body: (payload ?? null) as PlaylistTrackMutationResult['body'],
    cookie: res.cookie ?? res.body?.cookie
  }
}

/** 网络层抛错时库抛出的是 axios 错误或 { status, body }，统一成 ServiceResult */
export function normalizePlaylistTracksError(error: unknown): PlaylistTrackMutationResult {
  const rejected = error as { status?: number; body?: unknown } | null

  if (rejected && typeof rejected.body === 'object' && rejected.body !== null) {
    return {
      status: rejected.status ?? 500,
      body: rejected.body as PlaylistTrackMutationResult['body']
    }
  }

  return {
    status: 500,
    body: null,
    error: error instanceof Error ? error.message : String(error)
  }
}
