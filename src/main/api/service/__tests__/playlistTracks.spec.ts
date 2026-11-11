import { describe, expect, it } from 'vitest'
import {
  normalizePlaylistTracksError,
  normalizeTrackIds,
  unwrapPlaylistTracksResponse
} from '../playlistTracks'

describe('normalizeTrackIds', () => {
  it('accepts the bracketed form the renderer sends', () => {
    expect(normalizeTrackIds('[3360174158]')).toEqual(['3360174158'])
    expect(normalizeTrackIds('[1, 2]')).toEqual(['1', '2'])
  })

  it('accepts plain comma lists and numeric ids', () => {
    expect(normalizeTrackIds('1,2,3')).toEqual(['1', '2', '3'])
    expect(normalizeTrackIds(42)).toEqual(['42'])
  })

  it('accepts arrays', () => {
    expect(normalizeTrackIds([1, '2'])).toEqual(['1', '2'])
  })

  it('drops empty entries', () => {
    expect(normalizeTrackIds('')).toEqual([])
    expect(normalizeTrackIds('[]')).toEqual([])
  })
})

describe('unwrapPlaylistTracksResponse', () => {
  it('flattens the double-wrapped add response', () => {
    // playlist_tracks 的 add 分支返回 { status: 200, body: { ...res } }
    expect(
      unwrapPlaylistTracksResponse({
        status: 200,
        body: { status: 200, body: { trackIds: '[1]', code: 200, count: 13 }, cookie: ['x'] }
      })
    ).toEqual({ status: 200, body: { trackIds: '[1]', code: 200, count: 13 }, cookie: ['x'] })
  })

  it('keeps a plain single-wrapped response as-is', () => {
    expect(unwrapPlaylistTracksResponse({ status: 200, body: { code: 200, count: 12 } })).toEqual({
      status: 200,
      body: { code: 200, count: 12 },
      cookie: undefined
    })
  })

  it('exposes the duplicate-track business error instead of hiding it', () => {
    expect(
      unwrapPlaylistTracksResponse({
        status: 200,
        body: { status: 200, body: { message: '歌单内歌曲重复', code: 502 }, cookie: [] }
      }).body
    ).toEqual({ message: '歌单内歌曲重复', code: 502 })
  })

  it('survives a missing payload', () => {
    expect(unwrapPlaylistTracksResponse(undefined)).toEqual({
      status: 200,
      body: null,
      cookie: undefined
    })
  })
})

describe('normalizePlaylistTracksError', () => {
  it('keeps the server body when the library rejected with one', () => {
    expect(
      normalizePlaylistTracksError({ status: 401, body: { code: 401, message: '无权限操作歌单' } })
    ).toEqual({ status: 401, body: { code: 401, message: '无权限操作歌单' } })
  })

  it('turns a transport error into a 500 with the message', () => {
    expect(normalizePlaylistTracksError(new Error('socket hang up'))).toEqual({
      status: 500,
      body: null,
      error: 'socket hang up'
    })
  })
})
