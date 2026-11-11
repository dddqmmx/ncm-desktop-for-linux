import { promises as fs } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { Readable } from 'node:stream'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { BrowserWindow } from 'electron'
import type { SongDownloadRequest } from '../../../../shared/download'
import { DownloadService } from '../downloadService'

const mocks = vi.hoisted(() => ({
  songDownloadUrl: vi.fn(),
  songDownloadDetail: vi.fn(),
  showSaveDialog: vi.fn(),
  getPath: vi.fn(),
  get: vi.fn()
}))

vi.mock('electron', () => ({
  app: { getPath: mocks.getPath },
  dialog: { showSaveDialog: mocks.showSaveDialog }
}))
vi.mock('axios', () => ({ default: { get: mocks.get } }))
vi.mock('../musicService', () => ({
  MusicService: {
    song_download_url: mocks.songDownloadUrl,
    song_download_detail: mocks.songDownloadDetail
  }
}))

const request: SongDownloadRequest = {
  songId: 42,
  songName: 'A/B',
  artists: 'Artist',
  quality: 'hires',
  cookie: 'test-cookie'
}

describe('song file downloads', () => {
  let directory: string
  let destination: string
  const audio = Buffer.from('fLaC test audio bytes')
  const source = {
    level: 'hires',
    url: 'https://example.com/song.flac',
    type: 'flac',
    size: audio.length,
    code: 200
  }

  beforeEach(async () => {
    vi.resetAllMocks()
    directory = await fs.mkdtemp(path.join(os.tmpdir(), 'ncm-download-test-'))
    destination = path.join(directory, 'song.flac')
    mocks.getPath.mockReturnValue(directory)
    mocks.songDownloadUrl.mockResolvedValue({ status: 200, body: { code: 200, data: source } })
    mocks.showSaveDialog.mockResolvedValue({ canceled: false, filePath: destination })
    mocks.get.mockImplementation(async () => ({ data: Readable.from([audio]) }))
  })

  afterEach(async () => {
    await fs.rm(directory, { recursive: true, force: true })
  })

  it('saves complete audio with the selected quality, authentication and correct file extension', async () => {
    const parent = { isDestroyed: () => false } as BrowserWindow
    expect(await DownloadService.downloadSong(request, parent)).toEqual({
      status: 'completed',
      filePath: destination,
      quality: 'hires'
    })
    expect(mocks.songDownloadUrl).toHaveBeenCalledWith({
      id: 42,
      level: 'hires',
      cookie: 'test-cookie'
    })
    expect(mocks.showSaveDialog).toHaveBeenCalledWith(
      parent,
      expect.objectContaining({
        defaultPath: path.join(directory, 'A_B - Artist.flac'),
        filters: [{ name: 'FLAC 音频', extensions: ['flac'] }]
      })
    )
    expect(await fs.readFile(destination)).toEqual(audio)
    expect(await fs.readdir(directory)).toEqual(['song.flac'])
  })

  it('allows cancelling without starting a transfer or creating files', async () => {
    mocks.showSaveDialog.mockResolvedValue({ canceled: true })
    expect(await DownloadService.downloadSong(request, null)).toEqual({ status: 'cancelled' })
    expect(mocks.get).not.toHaveBeenCalled()
    expect(await fs.readdir(directory)).toEqual([])
  })

  it.each(['lossless', undefined])(
    'rejects downgraded or unconfirmed quality: %s',
    async (level) => {
      mocks.songDownloadUrl.mockResolvedValue({
        status: 200,
        body: { code: 200, data: { ...source, level } }
      })
      expect(await DownloadService.downloadSong(request, null)).toMatchObject({ status: 'error' })
      expect(mocks.showSaveDialog).not.toHaveBeenCalled()
      expect(mocks.get).not.toHaveBeenCalled()
    }
  )

  it('refreshes authenticated download permissions without using cached song metadata', async () => {
    mocks.songDownloadDetail
      .mockResolvedValueOnce({
        status: 200,
        body: {
          code: 200,
          songs: [{ id: 42 }],
          privileges: [{ id: 42, dlLevel: 'hires' }]
        }
      })
      .mockResolvedValueOnce({
        status: 200,
        body: {
          code: 200,
          songs: [{ id: 42 }],
          privileges: [{ id: 42, dlLevel: 'none' }]
        }
      })
    const first = await DownloadService.getQualityOptions({ songId: 42, cookie: 'account-a' })
    expect(
      first.status === 'ready' &&
        first.options.some((option) => option.quality === 'hires' && option.available)
    ).toBe(true)
    const expired = await DownloadService.getQualityOptions({ songId: 42, cookie: 'account-a' })
    expect(expired.status === 'ready' && expired.options.every((option) => !option.available)).toBe(
      true
    )
    expect(mocks.songDownloadDetail).toHaveBeenCalledTimes(2)
    expect(mocks.songDownloadDetail).toHaveBeenCalledWith({ ids: '42', cookie: 'account-a' })
  })

  it('reports missing permissions or failed requests instead of enabling every quality', async () => {
    mocks.songDownloadDetail.mockResolvedValue({
      status: 200,
      body: { code: 200, songs: [{ id: 42 }], privileges: [] }
    })
    expect(await DownloadService.getQualityOptions({ songId: 42 })).toMatchObject({
      status: 'error'
    })
    mocks.songDownloadDetail.mockResolvedValue({ status: 500, body: null, error: '网络断开' })
    expect(await DownloadService.getQualityOptions({ songId: 42 })).toEqual({
      status: 'error',
      message: '网络断开'
    })
  })

  it.each([
    { status: 200, body: { code: 403, message: '没有下载权限' } },
    { status: 200, body: { code: 200, data: { url: null } } },
    { status: 500, body: null, error: '网络连接失败' },
    { status: 200, body: { code: 200, data: { ...source, freeTrialInfo: { start: 0, end: 30 } } } }
  ])(
    'reports unavailable or trial-only audio before asking for a save location: %j',
    async (response) => {
      mocks.songDownloadUrl.mockResolvedValue(response)
      expect(await DownloadService.downloadSong(request, null)).toMatchObject({ status: 'error' })
      expect(mocks.showSaveDialog).not.toHaveBeenCalled()
      expect(mocks.get).not.toHaveBeenCalled()
    }
  )

  it.each(['incomplete', 'interrupted'])(
    'cleans up %s downloads and preserves an existing destination',
    async (failure) => {
      await fs.writeFile(destination, 'existing song')
      if (failure === 'incomplete') {
        mocks.get.mockResolvedValue({ data: Readable.from([Buffer.from('short')]) })
      } else {
        mocks.get.mockResolvedValue({
          data: Readable.from(
            (async function* () {
              yield Buffer.from('partial')
              throw new Error('connection reset')
            })()
          )
        })
      }
      expect(await DownloadService.downloadSong(request, null)).toMatchObject({ status: 'error' })
      expect(await fs.readFile(destination, 'utf8')).toBe('existing song')
      expect(await fs.readdir(directory)).toEqual(['song.flac'])
    }
  )

  it('deduplicates an in-flight download and allows another attempt after it settles', async () => {
    const first = DownloadService.downloadSong(request, null)
    expect(DownloadService.downloadSong(request, null)).toBe(first)
    await first
    expect(mocks.showSaveDialog).toHaveBeenCalledTimes(1)
    expect(mocks.get).toHaveBeenCalledTimes(1)

    await DownloadService.downloadSong(request, null)
    expect(mocks.get).toHaveBeenCalledTimes(2)
  })
})
