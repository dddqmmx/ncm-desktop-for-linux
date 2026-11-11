import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import type { CurrentSong } from '@renderer/types/player'
import type {
  SongDownloadResult,
  SongDownloadOptionsResult,
  DownloadQuality
} from '../../../shared/download'
import { useUserStore } from './userStore'

interface DownloadState {
  pending: boolean
  result?: SongDownloadResult
}

interface QualityOptionsState {
  loading: boolean
  result?: SongDownloadOptionsResult
}

export const useDownloadStore = defineStore('downloads', () => {
  const downloads = ref<Record<number, DownloadState>>({})
  const qualityOptions = ref<Record<number, QualityOptionsState>>({})
  const userStore = useUserStore()
  const requests = new Map<number, number>()
  let requestId = 0

  watch(
    () => userStore.cookie,
    () => {
      qualityOptions.value = {}
      requests.clear()
    },
    { flush: 'sync' }
  )

  const loadQualities = async (songId: number): Promise<void> => {
    if (qualityOptions.value[songId]?.loading) return
    const id = ++requestId
    const cookie = userStore.cookie
    requests.set(songId, id)
    qualityOptions.value[songId] = { loading: true }
    let result: SongDownloadOptionsResult
    try {
      result = await window.api.download_song_qualities({ songId, cookie })
    } catch (error) {
      result = {
        status: 'error',
        message: error instanceof Error ? error.message : '获取下载音质失败，请重试'
      }
    }
    if (requests.get(songId) !== id || cookie !== userStore.cookie) return
    qualityOptions.value[songId] = { loading: false, result }
    requests.delete(songId)
  }

  const download = async (song: CurrentSong, quality: DownloadQuality): Promise<void> => {
    if (downloads.value[song.id]?.pending) return
    const state = qualityOptions.value[song.id]
    if (
      state?.loading ||
      state?.result?.status !== 'ready' ||
      !state.result.options.some((option) => option.quality === quality && option.available)
    ) {
      downloads.value[song.id] = {
        pending: false,
        result: { status: 'error', message: '请选择当前账号可下载的音质' }
      }
      return
    }
    downloads.value[song.id] = { pending: true }
    let result: SongDownloadResult
    try {
      result = await window.api.download_song({
        songId: song.id,
        songName: song.name,
        artists: song.artists.map((artist) => artist.name).join(', '),
        quality,
        cookie: userStore.cookie
      })
    } catch (error) {
      result = {
        status: 'error',
        message: error instanceof Error ? error.message : '下载失败，请重试'
      }
    }
    downloads.value[song.id] = { pending: false, result }
  }

  return { downloads, qualityOptions, loadQualities, download }
})
