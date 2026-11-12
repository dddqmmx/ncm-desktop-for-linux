import { createPinia, setActivePinia } from 'pinia'
import { reactive } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Playlist } from '@renderer/types/userPlaylist'
import { MAX_PLAYLIST_DESCRIPTION_LENGTH, usePlaylistStore } from '../playlistStore'

const user = reactive({
  isLoggedIn: true,
  cookie: 'test-cookie',
  userInfo: { account: { id: 7 } } as { account: { id: number } } | undefined
})

vi.mock('../userStore', () => ({ useUserStore: () => user }))

const makePlaylist = (overrides: Partial<Playlist> = {}): Playlist =>
  ({
    id: 42,
    name: '原歌单',
    description: '原简介',
    specialType: 0,
    creator: { userId: 7 },
    tags: ['流行'],
    ...overrides
  }) as Playlist

const deferred = <T>(): { promise: Promise<T>; resolve: (value: T) => void } => {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

describe('playlistStore', () => {
  const api = {
    user_playlist: vi.fn(),
    playlist_update: vi.fn(),
    playlist_delete: vi.fn()
  }

  beforeEach(() => {
    setActivePinia(createPinia())
    user.isLoggedIn = true
    user.cookie = 'test-cookie'
    user.userInfo = { account: { id: 7 } }
    vi.stubGlobal('window', { api })
    api.user_playlist.mockResolvedValue({
      status: 200,
      body: { code: 200, playlist: [makePlaylist()] }
    })
    api.playlist_update.mockResolvedValue({ status: 200, body: { code: 200 } })
    api.playlist_delete.mockResolvedValue({ status: 200, body: { code: 200 } })
  })

  afterEach(() => {
    vi.resetAllMocks()
    vi.unstubAllGlobals()
  })

  it('saves metadata and updates the list used by the sidebar', async () => {
    const store = usePlaylistStore()
    await store.fetchPlaylists()

    const metadata = await store.updatePlaylist(store.playlists[0], {
      name: '  深夜    循环  ',
      description: '第一行\n第二行'
    })

    expect(api.playlist_update).toHaveBeenCalledWith({
      id: 42,
      name: '深夜 循环',
      desc: '第一行\n第二行',
      cookie: 'test-cookie'
    })
    expect(metadata).toEqual({ name: '深夜 循环', description: '第一行\n第二行' })
    expect(store.playlists[0]).toMatchObject({ ...metadata, tags: ['流行'] })
  })

  it('allows clearing the description', async () => {
    const store = usePlaylistStore()
    await store.fetchPlaylists()
    await store.updatePlaylist(store.playlists[0], { name: '歌单', description: '' })
    expect(store.playlists[0].description).toBe('')
    expect(api.playlist_update).toHaveBeenCalledWith(expect.objectContaining({ desc: '' }))
  })

  it('does not mutate local metadata when saving fails', async () => {
    const store = usePlaylistStore()
    await store.fetchPlaylists()
    api.playlist_update.mockResolvedValue({
      status: 200,
      body: { code: 403, message: '名称不合法' }
    })

    await expect(
      store.updatePlaylist(store.playlists[0], {
        name: '新名称',
        description: '新简介'
      })
    ).rejects.toThrow('名称不合法')
    expect(store.playlists[0]).toMatchObject({ name: '原歌单', description: '原简介' })
  })

  it('rejects blank names and descriptions exceeding the limit before submitting', async () => {
    const store = usePlaylistStore()
    await expect(
      store.updatePlaylist(makePlaylist(), {
        name: ' \t ',
        description: ''
      })
    ).rejects.toThrow('请输入歌单名称')
    await expect(
      store.updatePlaylist(makePlaylist(), {
        name: '歌单',
        description: '长'.repeat(MAX_PLAYLIST_DESCRIPTION_LENGTH + 1)
      })
    ).rejects.toThrow('歌单简介不能超过')
    expect(api.playlist_update).not.toHaveBeenCalled()
  })

  it.each([
    { creator: { userId: 8 } } as Partial<Playlist>,
    { specialType: 5 },
    { specialType: 10 }
  ])('rejects editing and deleting a playlist that cannot be managed: %j', async (overrides) => {
    const store = usePlaylistStore()
    const playlist = makePlaylist(overrides)
    expect(store.canManagePlaylist(playlist)).toBe(false)
    await expect(store.updatePlaylist(playlist, { name: '改名', description: '' })).rejects.toThrow(
      '只能管理'
    )
    await expect(store.deletePlaylist(playlist)).rejects.toThrow('只能管理')
    expect(api.playlist_update).not.toHaveBeenCalled()
    expect(api.playlist_delete).not.toHaveBeenCalled()
  })

  it('rejects mutations after logout', async () => {
    const store = usePlaylistStore()
    user.isLoggedIn = false
    await expect(store.deletePlaylist(makePlaylist())).rejects.toThrow('只能管理')
    expect(api.playlist_delete).not.toHaveBeenCalled()
  })

  it('removes only the deleted playlist after the server confirms success', async () => {
    const store = usePlaylistStore()
    store.playlists = [makePlaylist(), makePlaylist({ id: 43 })]
    await store.deletePlaylist(store.playlists[0])
    expect(api.playlist_delete).toHaveBeenCalledWith({ id: 42, cookie: 'test-cookie' })
    expect(store.playlists.map((item) => item.id)).toEqual([43])
  })

  it('keeps the playlist after a failed deletion and allows retrying', async () => {
    const store = usePlaylistStore()
    await store.fetchPlaylists()
    api.playlist_delete.mockRejectedValueOnce(new Error('网络断开'))
    await expect(store.deletePlaylist(store.playlists[0])).rejects.toThrow('网络断开')
    expect(store.playlists).toHaveLength(1)
    await store.deletePlaylist(store.playlists[0])
    expect(store.playlists).toHaveLength(0)
  })

  it('keeps the playlist when deletion returns a business error', async () => {
    const store = usePlaylistStore()
    await store.fetchPlaylists()
    api.playlist_delete.mockResolvedValue({
      status: 200,
      body: { code: 401, message: '无权限操作歌单' }
    })
    await expect(store.deletePlaylist(store.playlists[0])).rejects.toThrow('无权限操作歌单')
    expect(store.playlists).toHaveLength(1)
  })

  it('prevents duplicate requests and editing while deletion is in progress', async () => {
    const store = usePlaylistStore()
    const pending = deferred<unknown>()
    api.playlist_delete.mockReturnValue(pending.promise)
    const deletion = store.deletePlaylist(makePlaylist())
    await expect(store.deletePlaylist(makePlaylist())).rejects.toThrow('正在更新')
    await expect(
      store.updatePlaylist(makePlaylist(), { name: '改名', description: '' })
    ).rejects.toThrow('正在更新')
    pending.resolve({ status: 200, body: { code: 200 } })
    await deletion
    expect(api.playlist_delete).toHaveBeenCalledTimes(1)
    expect(api.playlist_update).not.toHaveBeenCalled()
  })

  it.each(['update', 'delete'])(
    'does not let an old list response undo a successful %s',
    async (operation) => {
      const store = usePlaylistStore()
      await store.fetchPlaylists()
      const pending = deferred<unknown>()
      api.user_playlist.mockReturnValue(pending.promise)
      const refresh = store.fetchPlaylists()
      if (operation === 'update') {
        await store.updatePlaylist(store.playlists[0], { name: '新名称', description: '' })
      } else {
        await store.deletePlaylist(store.playlists[0])
      }
      pending.resolve({ status: 200, body: { code: 200, playlist: [makePlaylist()] } })
      await refresh
      expect(store.playlists.map((item) => item.name)).toEqual(
        operation === 'update' ? ['新名称'] : []
      )
    }
  )

  it('clears the list on account change and ignores the previous account response', async () => {
    const store = usePlaylistStore()
    await store.fetchPlaylists()
    const pending = deferred<unknown>()
    api.user_playlist.mockReturnValue(pending.promise)
    const refresh = store.fetchPlaylists()
    user.userInfo = { account: { id: 8 } }
    expect(store.playlists).toEqual([])
    pending.resolve({ status: 200, body: { code: 200, playlist: [makePlaylist()] } })
    await refresh
    expect(store.playlists).toEqual([])
  })
})
