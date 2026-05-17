import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import type { CurrentSong } from '@renderer/types/player'
import type { Song, SongDetailResult } from '@renderer/types/songDetail'
import { createCurrentSongArtists, normalizeCurrentSong } from './player/utils'
import { useUserStore } from './userStore'

interface ServiceResult<T> {
  status: number
  body?: T | null
  error?: string
}

interface LikelistResult {
  ids?: unknown[]
  code?: number
}

interface LikeResult {
  code?: number
}

const isSuccess = <T>(res: ServiceResult<T>): boolean => {
  const code = (res.body as { code?: number } | null | undefined)?.code
  return res.status >= 200 && res.status < 300 && (code === undefined || code === 200)
}

const mapSongToCurrentSong = (song: Song): CurrentSong => ({
  id: song.id,
  name: song.name,
  artists: createCurrentSongArtists(song.ar),
  cover: song.al?.picUrl || '',
  duration: song.dt
})

export const useFavoriteStore = defineStore('favorites', () => {
  const favoriteSongs = ref<CurrentSong[]>([])
  const favoriteIds = ref<Set<number>>(new Set())
  const isLoading = ref(false)
  const errorMessage = ref('')
  const loadedUid = ref<number | null>(null)

  const userStore = useUserStore()

  const favoriteCount = computed(() => favoriteIds.value.size)

  const isFavorite = (songId: number | null | undefined): boolean => {
    if (typeof songId !== 'number') return false
    return favoriteIds.value.has(songId)
  }

  const setFavoriteIds = (ids: number[]): void => {
    favoriteIds.value = new Set(ids)
  }

  const syncFavoriteIdsFromSongs = (): void => {
    setFavoriteIds(favoriteSongs.value.map((song) => song.id))
  }

  const getUserId = async (): Promise<number | null> => {
    if (!userStore.isLoggedIn) return null

    if (!userStore.userInfo) {
      await userStore.getUserAccount()
    }

    return userStore.userInfo?.profile.userId ?? null
  }

  const clearFavorites = (): void => {
    favoriteSongs.value = []
    setFavoriteIds([])
    loadedUid.value = null
  }

  const fetchFavoriteSongs = async (force = false): Promise<void> => {
    const uid = await getUserId()
    if (!uid) {
      clearFavorites()
      return
    }

    if (!force && loadedUid.value === uid && favoriteIds.value.size > 0) return

    isLoading.value = true
    errorMessage.value = ''

    try {
      const likeListRes = (await window.api.likelist({
        uid,
        cookie: userStore.cookie
      })) as ServiceResult<LikelistResult>

      if (!isSuccess(likeListRes)) {
        throw new Error(likeListRes.error || '获取喜欢的音乐失败')
      }

      const ids = (likeListRes.body?.ids || [])
        .map((id) => Number(id))
        .filter((id) => Number.isFinite(id))

      setFavoriteIds(ids)
      loadedUid.value = uid

      if (ids.length === 0) {
        favoriteSongs.value = []
        return
      }

      const detailRes = (await window.api.song_detail({
        ids,
        cookie: userStore.cookie
      })) as ServiceResult<SongDetailResult>

      if (!isSuccess(detailRes) || !detailRes.body?.songs) {
        throw new Error(detailRes.error || '获取喜欢的音乐详情失败')
      }

      favoriteSongs.value = detailRes.body.songs.map(mapSongToCurrentSong)
      syncFavoriteIdsFromSongs()
    } catch (error) {
      console.error('读取喜欢的音乐失败', error)
      errorMessage.value = error instanceof Error ? error.message : '读取喜欢的音乐失败'
    } finally {
      isLoading.value = false
    }
  }

  const setCloudFavorite = async (song: CurrentSong, like: boolean): Promise<void> => {
    const normalizedSong = normalizeCurrentSong(song)
    if (!normalizedSong) return

    if (!(await getUserId())) {
      errorMessage.value = '登录后才能同步喜欢的音乐'
      return
    }

    errorMessage.value = ''
    const previousSongs = favoriteSongs.value
    const previousIds = favoriteIds.value

    if (like) {
      if (!favoriteIds.value.has(normalizedSong.id)) {
        favoriteSongs.value = [normalizedSong, ...favoriteSongs.value]
        setFavoriteIds([normalizedSong.id, ...Array.from(favoriteIds.value)])
      }
    } else {
      favoriteSongs.value = favoriteSongs.value.filter((item) => item.id !== normalizedSong.id)
      setFavoriteIds(Array.from(favoriteIds.value).filter((id) => id !== normalizedSong.id))
    }

    try {
      const res = (await window.api.like({
        id: normalizedSong.id,
        like,
        cookie: userStore.cookie
      })) as ServiceResult<LikeResult>

      if (!isSuccess(res)) {
        throw new Error(res.error || (like ? '喜欢歌曲失败' : '取消喜欢失败'))
      }
    } catch (error) {
      favoriteSongs.value = previousSongs
      favoriteIds.value = previousIds
      console.error('更新喜欢的音乐失败', error)
      errorMessage.value = error instanceof Error ? error.message : '更新喜欢的音乐失败'
    }
  }

  const addFavorite = (song: CurrentSong): Promise<void> => setCloudFavorite(song, true)

  const removeFavorite = async (songId: number): Promise<void> => {
    const existingSong =
      favoriteSongs.value.find((song) => song.id === songId) ||
      normalizeCurrentSong({ id: songId, name: '', artists: [], cover: '', duration: 0 })

    if (!existingSong) return
    await setCloudFavorite(existingSong, false)
  }

  const toggleFavorite = (song: CurrentSong): Promise<void> => {
    return setCloudFavorite(song, !isFavorite(song.id))
  }

  return {
    favoriteSongs,
    favoriteCount,
    isLoading,
    errorMessage,
    isFavorite,
    fetchFavoriteSongs,
    addFavorite,
    removeFavorite,
    toggleFavorite,
    clearFavorites
  }
})
