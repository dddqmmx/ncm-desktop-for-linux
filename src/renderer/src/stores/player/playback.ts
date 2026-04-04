import { defineStore } from 'pinia'
import { ref, watch, computed } from 'vue'
import { CurrentSong, Privilege } from '@renderer/types/player'
import { Song } from '@renderer/types/songDetail'
import { SongUrl, SoundQualityType } from '@renderer/types/song'
import { useUserStore } from '../userStore'
import { useConfigStore } from '../configStore'
import { usePlaylistStore } from './playlist'
import { prepareCachedSongSource } from '@renderer/utils/cache'
import { createCurrentSongArtists } from './utils'
import { getAvailableQualities, computePlayableLevel } from './quality'

export const usePlaybackStore = defineStore('playback', () => {
  const currentSong = ref<CurrentSong | null>(
    JSON.parse(localStorage.getItem('currentSong') || 'null')
  )
  const currentSongId = ref<number | null>(Number(localStorage.getItem('currentSongId')) || null)
  const currentTime = ref(Number(localStorage.getItem('currentTime') || 0))
  const isPlaying = ref(false)
  const isHistorySong = ref(true)
  const isSeeking = ref(false)
  const isSwitching = ref(false)

  const userStore = useUserStore()
  const configStore = useConfigStore()
  const playlistStore = usePlaylistStore()

  let progressTimer: ReturnType<typeof setInterval> | null = null
  let playToken = 0

  const duration = computed(() => currentSong.value?.duration || 0)
  const progressPercent = computed(() => {
    if (duration.value <= 0) return 0
    return ((currentTime.value % duration.value) / duration.value) * 100
  })

  // --- 内部逻辑 ---

  const syncProgress = async (): Promise<void> => {
    if (isSeeking.value) return
    try {
      const progressMs = await window.api.get_progress()
      if (progressMs !== undefined && progressMs !== null) {
        currentTime.value = progressMs
      }
    } catch (error) {
      console.error('同步进度失败:', error)
    }
  }

  const startTimer = (): void => {
    if (progressTimer) return
    progressTimer = setInterval(syncProgress, 100)
  }

  const stopTimer = (): void => {
    if (progressTimer) {
      clearInterval(progressTimer)
      progressTimer = null
    }
  }

  const getSongDetailData = async (
    id: number
  ): Promise<{ song: Song; privilege: Privilege } | undefined> => {
    try {
      const res = (await window.api.song_detail({ ids: [id] })) as {
        body?: { songs: Song[]; privileges: Privilege[] }
      }
      if (res.body?.songs?.[0] && res.body?.privileges?.[0]) {
        return {
          song: res.body.songs[0],
          privilege: res.body.privileges[0]
        }
      }
    } catch (e) {
      console.error('获取歌曲详情失败', e)
    }
    return undefined
  }

  const fetchSongUrl = async (song_id: number, level: SoundQualityType): Promise<string> => {
    try {
      const res = (await window.api.song_url({
        id: song_id,
        level: level,
        cookie: userStore.cookie
      })) as { body?: { data?: SongUrl[] } }
      return res.body?.data?.[0].url ?? ''
    } catch (e) {
      console.error('获取歌曲URL失败', e)
      return ''
    }
  }

  const setPlayerData = (song: Song, playing: boolean = true): void => {
    currentSong.value = {
      id: song.id,
      name: song.name,
      artists: createCurrentSongArtists(song.ar),
      cover: song.al.picUrl,
      duration: song.dt
    }
    currentSongId.value = song.id
    playlistStore.currentSongId = song.id
    isPlaying.value = playing
  }

  const waitForEnd = async (songId: number, token: number): Promise<void> => {
    try {
      await window.api.wait_finished()
      if (token !== playToken) return
      if (isSwitching.value) return
      if (currentSongId.value !== songId) return

      isPlaying.value = false
      isHistorySong.value = true
      stopTimer()
      currentTime.value = duration.value
      await playNext(true)
    } catch {
      // ignore
    }
  }

  // --- 公开操作 ---

  const playMusic = async (
    song_id: number,
    startTime: number = 0,
    forceRestart: boolean = false
  ): Promise<void> => {
    if (isSwitching.value) return

    if (!forceRestart && currentSongId.value === song_id && isPlaying.value) {
      return
    }

    isSwitching.value = true
    playToken++
    const token = playToken

    try {
      if (
        !forceRestart &&
        currentSongId.value === song_id &&
        !isPlaying.value &&
        !isHistorySong.value
      ) {
        await configStore.ensureConfiguredOutputDevice()
        await window.api.resume()
        isPlaying.value = true
        return
      }

      const detailData = await getSongDetailData(song_id)
      if (!detailData) {
        isSwitching.value = false
        return
      }

      const { song, privilege } = detailData
      const availableQualities = getAvailableQualities(song, privilege)
      const targetLevel = computePlayableLevel(availableQualities, configStore.soundQuality)

      const url = await fetchSongUrl(song_id, targetLevel)
      if (!url) {
        isSwitching.value = false
        return
      }

      try {
        await window.api.stop()
      } catch {
        // ignore
      }

      await configStore.ensureConfiguredOutputDevice()

      const playbackSource = await prepareCachedSongSource(song_id, targetLevel, url)
      if (playbackSource.type === 'file') {
        await window.api.play_file(playbackSource.value, startTime / 1000)
      } else if (playbackSource.cachePath && playbackSource.metadataPath) {
        await window.api.play_url_cached(
          playbackSource.value,
          playbackSource.cachePath,
          playbackSource.metadataPath,
          song.dt,
          playbackSource.cacheAheadSecs ?? configStore.songCacheAheadSecs,
          startTime / 1000
        )
      } else {
        await window.api.play_url(playbackSource.value, startTime / 1000)
      }

      setPlayerData(song, true)
      isHistorySong.value = false
      currentTime.value = startTime

      const exists = playlistStore.playlist.some((s) => s.id === song_id)
      if (!exists) {
        playlistStore.addToPlaylist({
          id: song.id,
          name: song.name,
          artists: createCurrentSongArtists(song.ar),
          cover: song.al.picUrl,
          duration: song.dt
        })
      }

      waitForEnd(song_id, token)
    } catch (error) {
      console.error('播放失败:', error)
    } finally {
      isSwitching.value = false
    }
  }

  const togglePlay = async (): Promise<void> => {
    if (isPlaying.value) {
      await window.api.pause()
      isPlaying.value = false
      return
    }
    if (isHistorySong.value && currentSongId.value) {
      await playMusic(currentSongId.value, currentTime.value)
      return
    }
    await configStore.ensureConfiguredOutputDevice()
    await window.api.resume()
    isPlaying.value = true
  }

  const seek = async (timeInMs: number): Promise<void> => {
    currentTime.value = timeInMs
    await window.api.seek(timeInMs / 1000)
  }

  const playNext = async (isAuto = false): Promise<void> => {
    const nextId = playlistStore.getNextSongId(isAuto)
    if (nextId !== null) {
      await playMusic(nextId)
    }
  }

  const playPrev = async (): Promise<void> => {
    const prevId = playlistStore.getPrevSongId()
    if (prevId !== null) {
      await playMusic(prevId)
    }
  }

  const initFromStorage = async (): Promise<void> => {
    if (!currentSongId.value) return

    if (currentSong.value && currentSong.value.id === currentSongId.value) {
      isHistorySong.value = true
      // 仍然尝试异步更新一下详情，但不阻塞
      getSongDetailData(currentSongId.value).then((data) => {
        if (data && data.song) {
          setPlayerData(data.song, false)
          isHistorySong.value = true
        }
      })
      return
    }

    const data = await getSongDetailData(currentSongId.value)
    if (data && data.song) {
      setPlayerData(data.song, false)
      isHistorySong.value = true
    }
  }

  const playAll = async (list: CurrentSong[], startIndex = 0): Promise<void> => {
    if (list.length === 0) return
    playlistStore.setPlaylist(list)
    const targetSong = list[startIndex]
    await playMusic(targetSong.id)
  }

  const stop = async (): Promise<void> => {
    try {
      await window.api.stop()
    } catch {
      // ignore
    }
    currentSong.value = null
    currentSongId.value = null
    isPlaying.value = false
    currentTime.value = 0
    isHistorySong.value = true
    stopTimer()
  }

  // --- 监听器 ---

  watch(currentSong, (song) => {
    if (song) {
      localStorage.setItem('currentSong', JSON.stringify(song))
    } else {
      localStorage.removeItem('currentSong')
    }
  })

  watch(currentSongId, (id) => {
    if (id !== null) {
      localStorage.setItem('currentSongId', id.toString())
      playlistStore.currentSongId = id
    } else {
      localStorage.removeItem('currentSongId')
      playlistStore.currentSongId = null
    }
  })

  watch(currentTime, (time) => {
    localStorage.setItem('currentTime', Math.floor(time).toString())
  })

  watch(
    isPlaying,
    (val) => {
      if (val) startTimer()
      else stopTimer()
    },
    { immediate: true }
  )

  return {
    currentSong,
    currentSongId,
    currentTime,
    isPlaying,
    isSeeking,
    duration,
    progressPercent,
    playMusic,
    togglePlay,
    seek,
    playNext,
    playPrev,
    stop,
    initFromStorage,
    playAll
  }
})
