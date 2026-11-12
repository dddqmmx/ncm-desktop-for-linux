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

export interface LocalMusicPathImportInput {
  filePath: string
  fileName?: string
  duration?: number
  cover?: string
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

function fileNameFromPath(filePath: string): string {
  const normalized = filePath.replace(/\\/g, '/')
  const segments = normalized.split('/')
  return segments[segments.length - 1] || filePath
}

function isPathUnderRoot(filePath: string, rootPath: string): boolean {
  const normalize = (value: string): string => value.replace(/\\/g, '/').replace(/\/+$/, '')
  const file = normalize(filePath)
  const root = normalize(rootPath)
  if (!file || !root) return false
  if (file === root) return true
  return file.startsWith(`${root}/`)
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

function sortLibrary(library: LocalSong[]): LocalSong[] {
  return [...library].sort((left, right) => left.name.localeCompare(right.name, 'zh-CN'))
}

export const useLocalMusicStore = defineStore('localMusic', () => {
  const songs = ref<LocalSong[]>(loadLibrary())
  const isImporting = ref(false)
  const isCheckingFiles = ref(false)
  const songCount = computed(() => songs.value.length)
  let fileCheckPromise: Promise<number> | null = null
  let durationCheckPromise: Promise<number> | null = null

  const removeMissingSongs = (): Promise<number> => {
    if (fileCheckPromise) return fileCheckPromise

    const checkFiles = window.api?.check_library_files
    if (typeof checkFiles !== 'function' || songs.value.length === 0) {
      return Promise.resolve(0)
    }

    const task = (async (): Promise<number> => {
      isCheckingFiles.value = true
      try {
        const { missingPaths } = await checkFiles(songs.value.map((song) => song.filePath))
        if (!missingPaths.length) return 0

        const missing = new Set(missingPaths)
        const before = songs.value.length
        songs.value = songs.value.filter((song) => !missing.has(song.filePath))
        return before - songs.value.length
      } catch (error) {
        console.error('检查本地音乐文件失败', error)
        return 0
      } finally {
        isCheckingFiles.value = false
      }
    })()

    fileCheckPromise = task
    void task.finally(() => {
      if (fileCheckPromise === task) fileCheckPromise = null
    })
    return task
  }

  const fillMissingDurations = (): Promise<number> => {
    if (durationCheckPromise) return durationCheckPromise

    const getDurations = window.api?.get_library_file_durations
    const targets = songs.value.filter((song) => song.duration <= 0)
    if (typeof getDurations !== 'function' || targets.length === 0) {
      return Promise.resolve(0)
    }

    const task = (async (): Promise<number> => {
      try {
        const results = await getDurations(targets.map((song) => song.filePath))
        const durations = new Map(
          results
            .filter((item) => Number.isFinite(item.duration) && item.duration > 0)
            .map((item) => [item.filePath, Math.round(item.duration)])
        )
        if (!durations.size) return 0

        let updated = 0
        songs.value = songs.value.map((song) => {
          const duration = durations.get(song.filePath)
          if (!duration || song.duration > 0) return song
          updated += 1
          return { ...song, duration }
        })
        return updated
      } catch (error) {
        console.error('读取本地音乐时长失败', error)
        return 0
      }
    })()

    durationCheckPromise = task
    void task.finally(() => {
      if (durationCheckPromise === task) durationCheckPromise = null
    })
    return task
  }

  const refreshLibraryFiles = async (): Promise<void> => {
    await removeMissingSongs()
    await fillMissingDurations()
  }

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
      songs.value = sortLibrary(songs.value)
      return result
    } finally {
      isImporting.value = false
    }
  }

  const importPaths = async (
    inputs: LocalMusicPathImportInput[]
  ): Promise<LocalMusicImportResult> => {
    const result: LocalMusicImportResult = { imported: 0, skipped: 0, failed: [] }
    const existingPaths = new Set(songs.value.map((song) => song.filePath))
    const existingSongs = new Map(songs.value.map((song) => [song.filePath, song]))

    isImporting.value = true
    try {
      const nextSongs = [...songs.value]

      for (const input of inputs) {
        const filePath = input.filePath?.trim()
        if (!filePath) {
          result.failed.push(input.fileName || 'unknown')
          continue
        }

        if (existingPaths.has(filePath)) {
          const duration = Math.max(0, Math.round(input.duration || 0))
          const existing = existingSongs.get(filePath)
          if (existing && existing.duration <= 0 && duration > 0) {
            existing.duration = duration
          }
          result.skipped += 1
          continue
        }

        const fileName = input.fileName?.trim() || fileNameFromPath(filePath)
        if (!AUDIO_EXTENSION.test(fileName) && !AUDIO_EXTENSION.test(filePath)) {
          result.skipped += 1
          continue
        }

        const { title, artist } = parseFileName(fileName)
        nextSongs.push({
          id: createLocalSongId(filePath),
          name: title,
          artists: [{ id: 0, name: artist }],
          cover: input.cover?.trim() || '',
          duration: Math.max(0, Math.round(input.duration || 0)),
          source: 'local',
          filePath,
          fileName
        })
        existingPaths.add(filePath)
        result.imported += 1
      }

      songs.value = sortLibrary(nextSongs)
      return result
    } finally {
      isImporting.value = false
    }
  }

  const removeSong = (id: number): void => {
    songs.value = songs.value.filter((song) => song.id !== id)
  }

  const removeSongsUnderPath = (rootPath: string): number => {
    const before = songs.value.length
    songs.value = songs.value.filter((song) => !isPathUnderRoot(song.filePath, rootPath))
    return before - songs.value.length
  }

  const clear = (): void => {
    songs.value = []
  }

  const reloadFromStorage = (): void => {
    songs.value = loadLibrary()
    void refreshLibraryFiles()
  }

  watch(songs, (library) => localStorage.setItem(STORAGE_KEY, JSON.stringify(library)), {
    deep: true
  })

  void refreshLibraryFiles()

  if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
    window.addEventListener('storage', (event) => {
      if (event.key === STORAGE_KEY) {
        reloadFromStorage()
      }
    })
  }

  return {
    songs,
    songCount,
    isImporting,
    isCheckingFiles,
    importFiles,
    importPaths,
    removeMissingSongs,
    fillMissingDurations,
    refreshLibraryFiles,
    removeSong,
    removeSongsUnderPath,
    clear,
    reloadFromStorage
  }
})
