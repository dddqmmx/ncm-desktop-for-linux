import { createPinia, setActivePinia } from 'pinia'
import { reactive } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  DOWNLOAD_QUALITIES,
  type SongDownloadResult,
  type SongDownloadOptionsResult
} from '../../../../shared/download'
import { useDownloadStore } from '../downloadStore'

const user = reactive({ cookie: 'test-cookie' })
vi.mock('../userStore', () => ({ useUserStore: () => user }))

const song = {
  id: 42,
  name: 'Song',
  artists: [{ id: 1, name: 'Artist' }],
  cover: '',
  duration: 180000
}
const available = {
  status: 'ready' as const,
  options: DOWNLOAD_QUALITIES.map(({ quality }) => ({
    quality,
    available: quality === 'standard' || quality === 'lossless'
  }))
}

describe('download state', () => {
  const downloadSong = vi.fn()
  const downloadQualities = vi.fn()

  beforeEach(() => {
    setActivePinia(createPinia())
    user.cookie = 'test-cookie'
    downloadQualities.mockResolvedValue(available)
    vi.stubGlobal('window', {
      api: { download_song: downloadSong, download_song_qualities: downloadQualities }
    })
  })

  afterEach(() => {
    vi.resetAllMocks()
    vi.unstubAllGlobals()
  })

  it('downloads the selected quality and shares state across menu instances', async () => {
    let finish!: (result: SongDownloadResult) => void
    downloadSong.mockReturnValue(
      new Promise<SongDownloadResult>((resolve) => {
        finish = resolve
      })
    )
    const firstMenu = useDownloadStore()
    await firstMenu.loadQualities(42)
    const pending = firstMenu.download(song, 'standard')
    const reopenedMenu = useDownloadStore()
    expect(reopenedMenu.downloads[42].pending).toBe(true)
    await reopenedMenu.download(song, 'lossless')
    expect(downloadSong).toHaveBeenCalledTimes(1)
    expect(downloadSong).toHaveBeenCalledWith({
      songId: 42,
      songName: 'Song',
      artists: 'Artist',
      quality: 'standard',
      cookie: 'test-cookie'
    })

    finish({ status: 'completed', filePath: '/music/song.mp3', quality: 'standard' })
    await pending
    expect(reopenedMenu.downloads[42]).toEqual({
      pending: false,
      result: { status: 'completed', filePath: '/music/song.mp3', quality: 'standard' }
    })
  })

  it('blocks unknown or unavailable quality selections', async () => {
    const store = useDownloadStore()
    await store.download(song, 'lossless')
    expect(downloadSong).not.toHaveBeenCalled()
    await store.loadQualities(42)
    await store.download(song, 'jymaster')
    expect(downloadSong).not.toHaveBeenCalled()
    expect(store.downloads[42].result).toMatchObject({ status: 'error' })
  })

  it('shows failures and permits retrying or cancelling', async () => {
    downloadSong.mockRejectedValueOnce(new Error('下载失败'))
    const store = useDownloadStore()
    await store.loadQualities(42)
    await store.download(song, 'lossless')
    expect(store.downloads[42]).toEqual({
      pending: false,
      result: { status: 'error', message: '下载失败' }
    })
    downloadSong.mockResolvedValueOnce({ status: 'cancelled' })
    await store.download(song, 'lossless')
    expect(store.downloads[42]).toEqual({ pending: false, result: { status: 'cancelled' } })
    expect(downloadSong).toHaveBeenCalledTimes(2)
  })

  it('refreshes rights on reopening the menu and handles membership expiry', async () => {
    const store = useDownloadStore()
    await store.loadQualities(42)
    downloadQualities.mockResolvedValue({
      status: 'ready',
      options: available.options.map((option) => ({ ...option, available: false }))
    })
    await store.loadQualities(42)
    await store.download(song, 'lossless')
    expect(downloadQualities).toHaveBeenCalledTimes(2)
    expect(downloadSong).not.toHaveBeenCalled()
  })

  it('invalidates rights immediately on account change and ignores a stale response', async () => {
    let finishOld!: (result: SongDownloadOptionsResult) => void
    const store = useDownloadStore()
    await store.loadQualities(42)
    downloadQualities.mockReturnValueOnce(
      new Promise<SongDownloadOptionsResult>((resolve) => {
        finishOld = resolve
      })
    )
    const oldRequest = store.loadQualities(42)
    user.cookie = 'new-account'
    expect(store.qualityOptions).toEqual({})
    await store.download(song, 'lossless')
    expect(downloadSong).not.toHaveBeenCalled()

    downloadQualities.mockResolvedValue({ status: 'ready', options: [] })
    await store.loadQualities(42)
    finishOld(available)
    await oldRequest
    expect(store.qualityOptions[42]).toEqual({
      loading: false,
      result: { status: 'ready', options: [] }
    })
    expect(downloadQualities).toHaveBeenLastCalledWith({ songId: 42, cookie: 'new-account' })
  })

  it('does not use old rights while refreshing and allows retrying a failed lookup', async () => {
    const store = useDownloadStore()
    await store.loadQualities(42)
    downloadQualities.mockRejectedValueOnce(new Error('获取权限失败'))
    const refresh = store.loadQualities(42)
    await store.download(song, 'lossless')
    expect(downloadSong).not.toHaveBeenCalled()
    await refresh
    expect(store.qualityOptions[42]).toEqual({
      loading: false,
      result: { status: 'error', message: '获取权限失败' }
    })
    await store.loadQualities(42)
    expect(store.qualityOptions[42]).toEqual({ loading: false, result: available })
  })
})
