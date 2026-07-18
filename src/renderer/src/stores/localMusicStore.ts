import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import type { LocalSong, PersistedCurrentSong } from '@renderer/types/player'
import { normalizeCurrentSong } from './player/utils'

const STORAGE_KEY = 'localMusicLibrary'
const AUDIO_EXTENSION = /\.(aac|aiff?|ape|flac|m4a|mp3|ogg|opus|wav|webm|wma)$/i

export interface LocalMusicImportResult {
  imported: number
  skipped: number
  failed: string[]
}

function loadLibrary(): LocalSong[] {
  try {
    const value = JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]') as unknown
    if (!Array.isArray(value)) return []

    return value
      .map((song) => normalizeCurrentSong(song as PersistedCurrentSong))
      .filter((song): song is LocalSong => song?.source === 'local' && !!song.filePath)
  } catch (error) {
    console.error('读取本地音乐列表失败', error)
    return []
  }
}

function createLocalSongId(filePath: string): number {
  let hash = 2166136261
  for (let index = 0; index < filePath.length; index += 1) {
    hash ^= filePath.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  return -(hash >>> 0 || 1)
}

function parseFileName(fileName: string): { title: string; artist: string } {
  const titleWithArtist = fileName.replace(/\.[^.]+$/, '').trim() || '未知歌曲'
  const separatorIndex = titleWithArtist.indexOf(' - ')
  if (separatorIndex <= 0 || separatorIndex >= titleWithArtist.length - 3) {
    return { title: titleWithArtist, artist: '未知艺术家' }
  }

  return {
    artist: titleWithArtist.slice(0, separatorIndex).trim(),
    title: titleWithArtist.slice(separatorIndex + 3).trim()
  }
}

function readAudioDuration(file: File): Promise<number> {
  return new Promise((resolve) => {
    const objectUrl = URL.createObjectURL(file)
    const audio = new Audio()
    let settled = false

    const finish = (duration = 0): void => {
      if (settled) return
      settled = true
      window.clearTimeout(timeout)
      audio.removeAttribute('src')
      audio.load()
      URL.revokeObjectURL(objectUrl)
      resolve(duration)
    }

    const timeout = window.setTimeout(() => finish(), 8_000)
    audio.preload = 'metadata'
    audio.addEventListener('loadedmetadata', () => {
      const duration = Number.isFinite(audio.duration) ? Math.round(audio.duration * 1000) : 0
      finish(duration)
    })
    audio.addEventListener('error', () => finish())
    audio.src = objectUrl
    audio.load()
  })
}

export const useLocalMusicStore = defineStore('localMusic', () => {
  const songs = ref<LocalSong[]>(loadLibrary())
  const isImporting = ref(false)
  const songCount = computed(() => songs.value.length)

  const importFiles = async (files: File[]): Promise<LocalMusicImportResult> => {
    const result: LocalMusicImportResult = { imported: 0, skipped: 0, failed: [] }
    const candidates = files.filter(
      (file) => file.type.startsWith('audio/') || AUDIO_EXTENSION.test(file.name)
    )

    isImporting.value = true
    try {
      for (const file of candidates) {
        try {
          const filePath = window.api.get_path_for_file(file)
          if (!filePath) {
            result.failed.push(file.name)
            continue
          }
          if (songs.value.some((song) => song.filePath === filePath)) {
            result.skipped += 1
            continue
          }

          const { title, artist } = parseFileName(file.name)
          const duration = await readAudioDuration(file)
          songs.value.push({
            id: createLocalSongId(filePath),
            name: title,
            artists: [{ id: 0, name: artist }],
            cover: '',
            duration,
            source: 'local',
            filePath,
            fileName: file.name
          })
          result.imported += 1
        } catch (error) {
          console.error(`导入本地音乐失败: ${file.name}`, error)
          result.failed.push(file.name)
        }
      }

      result.skipped += files.length - candidates.length
      songs.value.sort((left, right) => left.name.localeCompare(right.name, 'zh-CN'))
      return result
    } finally {
      isImporting.value = false
    }
  }

  const removeSong = (id: number): void => {
    songs.value = songs.value.filter((song) => song.id !== id)
  }

  const clear = (): void => {
    songs.value = []
  }

  watch(songs, (library) => localStorage.setItem(STORAGE_KEY, JSON.stringify(library)), {
    deep: true
  })

  return {
    songs,
    songCount,
    isImporting,
    importFiles,
    removeSong,
    clear
  }
})
