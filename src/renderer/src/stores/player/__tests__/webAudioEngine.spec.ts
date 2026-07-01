import { describe, expect, it } from 'vitest'
import { toAudioUrl } from '../webAudioEngine'
import type { CachedSongSource } from '@renderer/types/cache'

const LINUX_PATH = '/home/user/ncm-desktop-for-linux/cache/song/123456_exh.mp3'
const WINDOWS_PATH = 'C:\\Users\\user\\ncm-desktop-for-linux\\cache\\song\\123456_exh.mp3'

function source(type: 'file' | 'url', value: string): CachedSongSource {
  return { type, value }
}

describe('webAudioEngine URL 解析', () => {
  it('命中本地缓存 (file) 时转成 ncm-cache:// 协议 URL', () => {
    const result = toAudioUrl(source('file', LINUX_PATH))
    expect(result).toBe(`ncm-cache://asset?path=${encodeURIComponent(LINUX_PATH)}`)
  })

  it('Windows 本地缓存路径也能转成正确的 ncm-cache URL', () => {
    const result = toAudioUrl(source('file', WINDOWS_PATH))
    expect(result).toBe(`ncm-cache://asset?path=${encodeURIComponent(WINDOWS_PATH)}`)
  })

  it('网络 URL (url) 保持原样', () => {
    const http = source(
      'url',
      'https://m701.music.126.net/20260101/abc/def.mp3?authSecret=xxx'
    )
    expect(toAudioUrl(http)).toBe(http.value)
  })

  it('已经是 ncm-cache 协议的 URL 保持原样', () => {
    const cached = source('url', 'ncm-cache://asset?path=%2Ffoo%2Fbar.mp3')
    expect(toAudioUrl(cached)).toBe(cached.value)
  })

  it('data/blob URL 保持原样', () => {
    expect(toAudioUrl(source('url', 'data:audio/mp3;base64,AAAA'))).toBe(
      'data:audio/mp3;base64,AAAA'
    )
    expect(toAudioUrl(source('url', 'blob:uuid'))).toBe('blob:uuid')
  })

  it('带空格的本地路径能被正确 encode', () => {
    const pathWithSpace = '/home/user/My Music/song.mp3'
    expect(toAudioUrl(source('file', pathWithSpace))).toBe(
      `ncm-cache://asset?path=${encodeURIComponent(pathWithSpace)}`
    )
  })
})
