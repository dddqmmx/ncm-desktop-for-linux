import { describe, expect, it } from 'vitest'
import { normalizePlaylistUpdateResult, playlistUpdateRequest } from '../playlistMetadata'

describe('playlist metadata requests', () => {
  it('preserves quotes, backslashes and multiline descriptions without changing tags', () => {
    const name = '我的 "Live" \\ 收藏'
    const desc = '第一行\n第二行："夜晚" \\ 现场'
    const request = playlistUpdateRequest({ id: 42, name, desc, cookie: 'test-cookie' })

    expect(JSON.parse(request['/api/playlist/update/name'] as string)).toEqual({ id: 42, name })
    expect(JSON.parse(request['/api/playlist/desc/update'] as string)).toEqual({ id: 42, desc })
    expect(request.cookie).toBe('test-cookie')
    expect(request).not.toHaveProperty('/api/playlist/tags/update')
  })

  it('explicitly sends an empty description so it can be cleared', () => {
    const request = playlistUpdateRequest({ id: 42, name: '歌单', desc: '' })
    expect(JSON.parse(request['/api/playlist/desc/update'] as string).desc).toBe('')
  })
})

describe('playlist metadata responses', () => {
  const success = {
    status: 200,
    body: {
      code: 200,
      '/api/playlist/update/name': { code: 200 },
      '/api/playlist/desc/update': { code: 200 }
    },
    cookie: ['test-cookie']
  }

  it('reports success only when both fields have been saved', () => {
    expect(normalizePlaylistUpdateResult(success)).toEqual({
      status: 200,
      body: { code: 200 },
      cookie: ['test-cookie']
    })
  })

  it.each(['/api/playlist/update/name', '/api/playlist/desc/update'])(
    'reports a failed subrequest at %s even if the batch returns 200',
    (path) => {
      const result = normalizePlaylistUpdateResult({
        ...success,
        body: { ...success.body, [path]: { code: 403, msg: '无权限操作歌单' } }
      })
      expect(result.body).toEqual({ code: 403, message: '无权限操作歌单' })
    }
  )

  it('rejects an incomplete batch response', () => {
    expect(normalizePlaylistUpdateResult({ status: 200, body: { code: 200 } }).body).toEqual({
      code: 502,
      message: '歌单信息未完整保存，请重试'
    })
  })

  it('preserves transport errors and top-level business failures', () => {
    for (const result of [
      { status: 500, body: null, error: '网络异常' },
      { status: 200, body: { code: 401, message: '请先登录' } }
    ]) {
      expect(normalizePlaylistUpdateResult(result)).toEqual(result)
    }
  })
})
