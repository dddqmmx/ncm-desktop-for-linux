import { afterEach, describe, expect, it, vi } from 'vitest'
import { preparePlaybackCache, startPlaybackBackgroundCache } from '../cache'

describe('playback cache helpers', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('prepares native cache sources without starting a background cache request', async () => {
    vi.stubGlobal('window', {
      api: {
        prepare_cached_song_source: vi.fn(async () => ({
          type: 'url',
          value: 'https://example.com/song.flac',
          cachePath: '/cache/song.flac.part',
          metadataPath: '/cache/song.flac.json'
        }))
      }
    } as unknown as Window & typeof globalThis)

    const cache = await preparePlaybackCache({
      engine: 'native',
      songId: 1,
      quality: 'lossless',
      url: 'https://example.com/song.flac',
      expectedBytes: 1024,
      durationMs: 180000
    })

    expect(cache.source.type).toBe('url')
    expect(cache.metadataPath).toBe('/cache/song.flac.json')
    expect(cache.initialBufferedPercent).toBe(0)
    expect(cache.backgroundCacheRequest).toBeUndefined()
    expect(startPlaybackBackgroundCache(cache)).toBeNull()
  })

  it('creates a background cache request for WebAPI network sources', async () => {
    const cacheSongSource = vi.fn(async () => ({
      type: 'file',
      value: '/cache/song.flac'
    }))
    vi.stubGlobal('window', {
      api: {
        prepare_cached_song_source: vi.fn(async () => ({
          type: 'url',
          value: 'https://example.com/song.flac',
          metadataPath: '/cache/song.flac.json'
        })),
        cache_song_source: cacheSongSource
      }
    } as unknown as Window & typeof globalThis)

    const cache = await preparePlaybackCache({
      engine: 'webapi',
      songId: 1,
      quality: 'lossless',
      url: 'https://example.com/song.flac',
      expectedBytes: 1024,
      durationMs: 180000
    })
    const backgroundCache = startPlaybackBackgroundCache(cache)

    await expect(backgroundCache).resolves.toEqual({
      type: 'file',
      value: '/cache/song.flac'
    })
    expect(cacheSongSource).toHaveBeenCalledWith({
      songId: 1,
      quality: 'lossless',
      url: 'https://example.com/song.flac',
      expectedBytes: 1024,
      durationMs: 180000
    })
  })

  it('does not background cache WebAPI sources that already hit a local file', async () => {
    vi.stubGlobal('window', {
      api: {
        prepare_cached_song_source: vi.fn(async () => ({
          type: 'file',
          value: '/cache/song.flac',
          metadataPath: '/cache/song.flac.json'
        })),
        cache_song_source: vi.fn()
      }
    } as unknown as Window & typeof globalThis)

    const cache = await preparePlaybackCache({
      engine: 'webapi',
      songId: 1,
      quality: 'lossless',
      url: 'https://example.com/song.flac'
    })

    expect(cache.source.type).toBe('file')
    expect(cache.initialBufferedPercent).toBe(100)
    expect(startPlaybackBackgroundCache(cache)).toBeNull()
  })
})
