import { describe, expect, it } from 'vitest'
import { apiErrorMessage, isApiSuccess } from '../api'

describe('isApiSuccess', () => {
  it('accepts a 2xx response with a success body code', () => {
    expect(isApiSuccess({ status: 200, body: { code: 200 } })).toBe(true)
  })

  it('treats the plain 2xx acknowledgement as success', () => {
    expect(isApiSuccess({ status: 200, body: null })).toBe(false)
    expect(isApiSuccess({ status: 200, body: {} })).toBe(true)
    expect(isApiSuccess({ status: 204, body: {} })).toBe(true)
  })

  it('rejects rejected business responses even if the transport looked fine', () => {
    // 歌单无权限：lib 以 status=401 返回，而不是抛异常
    expect(isApiSuccess({ status: 401, body: { code: 401, message: '无权限操作歌单' } })).toBe(
      false
    )
    // 部分错误码（800/801/802）会带着 HTTP 200 一起回来
    expect(isApiSuccess({ status: 200, body: { code: 801 } })).toBe(false)
  })

  it('rejects transport failures', () => {
    expect(isApiSuccess({ status: 500, body: null, error: 'connect ECONNREFUSED' })).toBe(false)
    expect(isApiSuccess(undefined)).toBe(false)
  })
})

describe('apiErrorMessage', () => {
  it('surfaces the server message so the reason is visible to the user', () => {
    expect(
      apiErrorMessage({ status: 401, body: { code: 401, message: '无权限操作歌单' } }, '添加失败')
    ).toBe('无权限操作歌单')
  })

  it('falls back to msg when message is missing', () => {
    expect(
      apiErrorMessage({ status: 401, body: { code: 401, msg: '歌单不存在' } }, '添加失败')
    ).toBe('歌单不存在')
  })

  it('reports a network problem when the request never produced a body', () => {
    expect(apiErrorMessage({ status: 500, body: null, error: 'socket hang up' }, '添加失败')).toBe(
      'socket hang up'
    )
    expect(apiErrorMessage({ status: 500, body: null }, '添加失败')).toBe('网络异常，请稍后重试')
  })

  it('uses the caller fallback when the server gives nothing useful', () => {
    expect(apiErrorMessage({ status: 401, body: { code: 401 } }, '添加失败，请重试')).toBe(
      '添加失败，请重试'
    )
    expect(apiErrorMessage(undefined, '添加失败，请重试')).toBe('添加失败，请重试')
  })
})
