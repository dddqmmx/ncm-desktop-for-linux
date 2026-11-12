export interface PlaylistUpdateParams {
  id: number
  name: string
  desc: string
  cookie?: string
}

interface MutationBody {
  code?: number
  message?: string
  msg?: string
}

interface MutationResult {
  status: number
  body: MutationBody | null
  cookie?: string[]
  error?: string
}

const UPDATE_PATHS = ['/api/playlist/update/name', '/api/playlist/desc/update'] as const

export function playlistUpdateRequest(params: PlaylistUpdateParams): Record<string, unknown> {
  // 库的 playlist_update 手动拼接 JSON，名称中的引号和多行简介会破坏请求。
  // 只提交这次编辑的字段，保留已有标签。
  return {
    cookie: params.cookie,
    [UPDATE_PATHS[0]]: JSON.stringify({ id: params.id, name: params.name }),
    [UPDATE_PATHS[1]]: JSON.stringify({ id: params.id, desc: params.desc })
  }
}

export function normalizePlaylistUpdateResult(result: MutationResult): MutationResult {
  if (result.status < 200 || result.status >= 300 || !result.body) return result
  if (typeof result.body.code === 'number' && result.body.code !== 200) return result

  const body = result.body as MutationBody & Record<string, MutationBody | undefined>
  for (const path of UPDATE_PATHS) {
    const fieldResult = body[path]
    // batch 的外层 code=200 并不代表每个字段都成功，缺失结果也不能视为成功。
    if (fieldResult?.code !== 200) {
      return {
        ...result,
        body: {
          code: fieldResult?.code && fieldResult.code >= 300 ? fieldResult.code : 502,
          message: fieldResult?.message || fieldResult?.msg || '歌单信息未完整保存，请重试'
        }
      }
    }
  }

  return { ...result, body: { code: 200 } }
}
