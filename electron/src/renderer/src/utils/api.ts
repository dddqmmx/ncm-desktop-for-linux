/**
 * 主进程统一返回 { status, body, cookie, error }（见 main/api/service/musicService.ts
 * 的 responseHandler）。业务失败时状态码来自网易云的 code（401 无权限、403、404…），
 * 也可能被兜底成 500。只有把 status/body.code 一起看才能判断是否真的成功——
 * 只看 IPC 有没有抛异常是不够的，IPC 本身几乎不会抛。
 */
export interface ServiceResult<T = unknown> {
  status: number
  body?: T | null
  cookie?: string[]
  error?: string
}

/** 传输层失败（重试耗尽、断网）兜底文案 */
const NETWORK_FAILURE_MESSAGE = '网络异常，请稍后重试'

/**
 * 成功判定：HTTP 状态 2xx + 有 body + body.code 不是 3xx 及以上。
 * 网易云对部分错误码（800/801/802…）返回 HTTP 200，因此 body.code 必须一起校验。
 */
export function isApiSuccess<T>(res: ServiceResult<T> | null | undefined): boolean {
  if (!res) return false
  if (!(res.status >= 200 && res.status < 300)) return false
  if (!res.body) return false

  const code = (res.body as { code?: unknown }).code
  if (typeof code === 'number' && code >= 300) return false

  return true
}

/**
 * 失败原因：优先用服务端给的中文提示（如「无权限操作歌单」），
 * 其次是传输层错误，最后才用调用方提供的兜底文案。
 */
export function apiErrorMessage(
  res: ServiceResult<unknown> | null | undefined,
  fallback: string
): string {
  const body = res?.body as { message?: unknown; msg?: unknown } | null | undefined
  for (const candidate of [body?.message, body?.msg, res?.error]) {
    if (typeof candidate === 'string' && candidate.trim()) return candidate.trim()
  }
  if (res && !res.body) return NETWORK_FAILURE_MESSAGE
  return fallback
}

/**
 * `/api/playlist/manipulate/tracks` 用 code 502 表示「歌单内歌曲重复」，
 * 这是用户可见的正常结果（歌已经在歌单里），不该当成失败提示。
 */
export function isDuplicateTrackResult(res: ServiceResult<unknown> | null | undefined): boolean {
  const body = res?.body as { code?: unknown; message?: unknown } | null | undefined
  return body?.code === 502 || body?.message === '歌单内歌曲重复'
}
