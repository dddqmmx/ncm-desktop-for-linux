import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import type { Playlist, PlaylistResponse } from '@renderer/types/userPlaylist'
import { apiErrorMessage, isApiSuccess, type ServiceResult } from '@renderer/utils/api'
import { normalizePlaylistName } from '@renderer/utils/playlistName'
import { useUserStore } from './userStore'

export type EditablePlaylist = Pick<Playlist, 'id' | 'name' | 'description' | 'specialType'> & {
  creator: { userId: number }
}

export interface PlaylistMetadata {
  name: string
  description: string
}

export const MAX_PLAYLIST_DESCRIPTION_LENGTH = 1000

export const usePlaylistStore = defineStore('playlists', () => {
  const userStore = useUserStore()
  const playlists = ref<Playlist[]>([])
  let revision = 0
  const pendingIds = new Set<number>()

  const clearPlaylists = (): void => {
    revision++
    playlists.value = []
  }

  watch(() => userStore.userInfo?.account.id, clearPlaylists, { flush: 'sync' })

  const canManagePlaylist = (playlist: EditablePlaylist): boolean =>
    userStore.isLoggedIn &&
    playlist.creator.userId === userStore.userInfo?.account.id &&
    playlist.specialType === 0

  const fetchPlaylists = async (): Promise<void> => {
    const uid = userStore.userInfo?.account.id
    if (!uid) {
      clearPlaylists()
      return
    }
    const requestRevision = ++revision
    try {
      const res = (await window.api.user_playlist({
        uid,
        cookie: userStore.cookie
      })) as ServiceResult<PlaylistResponse>
      if (requestRevision !== revision || uid !== userStore.userInfo?.account.id) return
      if (!isApiSuccess(res) || !Array.isArray(res.body?.playlist)) {
        throw new Error(apiErrorMessage(res, '获取歌单失败'))
      }
      playlists.value = res.body.playlist
    } catch (error) {
      console.error('获取歌单失败:', error)
    }
  }

  const mutatePlaylist = async (
    playlist: EditablePlaylist,
    request: () => Promise<unknown>,
    apply: () => void,
    fallback: string
  ): Promise<void> => {
    if (!canManagePlaylist(playlist)) throw new Error('只能管理自己创建的普通歌单')
    if (pendingIds.has(playlist.id)) throw new Error('歌单正在更新，请稍后重试')

    const uid = userStore.userInfo?.account.id
    pendingIds.add(playlist.id)
    try {
      const res = (await request()) as ServiceResult
      if (!isApiSuccess(res)) throw new Error(apiErrorMessage(res, fallback))
      if (uid === userStore.userInfo?.account.id) {
        revision++
        apply()
      }
    } finally {
      pendingIds.delete(playlist.id)
    }
  }

  const updatePlaylist = async (
    playlist: EditablePlaylist,
    metadata: PlaylistMetadata
  ): Promise<PlaylistMetadata> => {
    const name = normalizePlaylistName(metadata.name)
    const description = metadata.description.trim()
    if (!name) throw new Error('请输入歌单名称')
    if (description.length > MAX_PLAYLIST_DESCRIPTION_LENGTH) {
      throw new Error(`歌单简介不能超过 ${MAX_PLAYLIST_DESCRIPTION_LENGTH} 个字`)
    }

    await mutatePlaylist(
      playlist,
      () =>
        window.api.playlist_update({
          id: playlist.id,
          name,
          desc: description,
          cookie: userStore.cookie
        }),
      () => {
        const cached = playlists.value.find((item) => item.id === playlist.id)
        if (cached) Object.assign(cached, { name, description })
      },
      '保存歌单失败，请重试'
    )
    return { name, description }
  }

  const deletePlaylist = (playlist: EditablePlaylist): Promise<void> =>
    mutatePlaylist(
      playlist,
      () => window.api.playlist_delete({ id: playlist.id, cookie: userStore.cookie }),
      () => {
        playlists.value = playlists.value.filter((item) => item.id !== playlist.id)
      },
      '删除歌单失败，请重试'
    )

  return {
    playlists,
    canManagePlaylist,
    fetchPlaylists,
    clearPlaylists,
    updatePlaylist,
    deletePlaylist
  }
})
