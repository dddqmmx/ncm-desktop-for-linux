import { app, dialog, type BrowserWindow, type SaveDialogOptions } from 'electron'
import axios from 'axios'
import { createWriteStream, promises as fs } from 'node:fs'
import path from 'node:path'
import { randomUUID } from 'node:crypto'
import { pipeline } from 'node:stream/promises'
import type { Readable } from 'node:stream'
import {
  DOWNLOAD_QUALITIES,
  isDownloadQuality,
  type SongDownloadRequest,
  type SongDownloadResult,
  type SongDownloadOptionsRequest,
  type SongDownloadOptionsResult
} from '../../../shared/download'
import { MusicService } from './musicService'
import {
  getDownloadQualityOptions,
  type DownloadPrivilege,
  type DownloadSongDetail
} from './downloadQuality'

interface DownloadSource {
  level?: string
  url?: string | null
  type?: string
  encodeType?: string
  size?: number
  code?: number
  message?: string
  freeTrialInfo?: unknown
}

interface DownloadResponse {
  status: number
  error?: string
  body: {
    code?: number
    message?: string
    msg?: string
    data?: DownloadSource | DownloadSource[]
  } | null
}

const AUDIO_EXTENSIONS = new Set(['mp3', 'flac', 'm4a', 'aac', 'wav', 'ogg', 'opus'])
const activeDownloads = new Map<number, Promise<SongDownloadResult>>()

function downloadExtension(source: DownloadSource): string {
  for (const format of [
    source.type,
    source.encodeType,
    path.extname(new URL(source.url!).pathname)
  ]) {
    const extension = format?.toLowerCase().replace(/^\./, '')
    if (extension && AUDIO_EXTENSIONS.has(extension)) return extension
  }
  return 'mp3'
}

function downloadFileName(payload: SongDownloadRequest, extension: string): string {
  const name = [payload.songName, payload.artists].filter(Boolean).join(' - ')
  const safeName = Array.from(name)
    .filter((character) => character.charCodeAt(0) >= 32)
    .join('')
    .replace(/[<>:"/\\|?*]/g, '_')
    .replace(/^\.+|[.\s]+$/g, '')
    .trim()
  return `${Array.from(safeName).slice(0, 60).join('') || payload.songId}.${extension}`
}

async function saveSong(
  payload: SongDownloadRequest,
  parent: BrowserWindow | null
): Promise<SongDownloadResult> {
  let temporaryPath: string | undefined
  try {
    if (!Number.isSafeInteger(payload.songId) || payload.songId <= 0) {
      throw new Error('无效的歌曲')
    }
    if (!isDownloadQuality(payload.quality)) throw new Error('请选择有效的下载音质')

    const response = (await MusicService.song_download_url({
      id: payload.songId,
      level: payload.quality as Parameters<typeof MusicService.song_download_url>[0]['level'],
      cookie: payload.cookie
    })) as DownloadResponse
    const body = response.body
    const source = Array.isArray(body?.data) ? body.data[0] : body?.data
    if (
      response.status !== 200 ||
      (body?.code !== undefined && body.code !== 200) ||
      (source?.code !== undefined && source.code !== 200) ||
      !source?.url ||
      source.freeTrialInfo
    ) {
      throw new Error(
        source?.message ||
          body?.message ||
          body?.msg ||
          response.error ||
          '该歌曲暂时无法下载，请检查登录状态、下载权限或音质设置'
      )
    }
    if (!['http:', 'https:'].includes(new URL(source.url).protocol)) {
      throw new Error('下载地址无效')
    }
    if (source.level !== payload.quality) {
      const requested = DOWNLOAD_QUALITIES.find((item) => item.quality === payload.quality)!.label
      const actual = DOWNLOAD_QUALITIES.find((item) => item.quality === source.level)?.label
      throw new Error(
        actual
          ? `所选${requested}暂不可下载，当前仅返回${actual}，请重新选择音质`
          : '未能确认下载音质，请刷新音质列表后重试'
      )
    }

    const extension = downloadExtension(source)
    const options: SaveDialogOptions = {
      title: '下载歌曲',
      defaultPath: path.join(app.getPath('downloads'), downloadFileName(payload, extension)),
      filters: [{ name: `${extension.toUpperCase()} 音频`, extensions: [extension] }]
    }
    const selection =
      parent && !parent.isDestroyed()
        ? await dialog.showSaveDialog(parent, options)
        : await dialog.showSaveDialog(options)
    if (selection.canceled || !selection.filePath) return { status: 'cancelled' }

    const filePath = selection.filePath
    temporaryPath = path.join(path.dirname(filePath), `.ncm-download-${randomUUID()}.part`)
    const audio = await axios.get<Readable>(source.url, {
      responseType: 'stream',
      timeout: 60_000
    })
    await pipeline(audio.data, createWriteStream(temporaryPath, { flags: 'wx' }))
    const { size } = await fs.stat(temporaryPath)
    if (
      size === 0 ||
      (typeof source.size === 'number' && source.size > 0 && size !== source.size)
    ) {
      throw new Error('歌曲下载不完整，请重试')
    }
    await fs.rename(temporaryPath, filePath)
    return { status: 'completed', filePath, quality: payload.quality }
  } catch (error) {
    return { status: 'error', message: error instanceof Error ? error.message : '下载失败，请重试' }
  } finally {
    if (temporaryPath) await fs.rm(temporaryPath, { force: true }).catch(() => undefined)
  }
}

export const DownloadService = {
  async getQualityOptions(payload: SongDownloadOptionsRequest): Promise<SongDownloadOptionsResult> {
    try {
      if (!Number.isSafeInteger(payload.songId) || payload.songId <= 0) {
        throw new Error('无效的歌曲')
      }
      const response = (await MusicService.song_download_detail({
        ids: String(payload.songId),
        cookie: payload.cookie
      })) as {
        status: number
        error?: string
        body: {
          code?: number
          message?: string
          songs?: DownloadSongDetail[]
          privileges?: DownloadPrivilege[]
        } | null
      }
      const song = response.body?.songs?.find((item) => item.id === payload.songId)
      const privilege = response.body?.privileges?.find((item) => item.id === payload.songId)
      if (response.status !== 200 || response.body?.code !== 200 || !song || !privilege) {
        throw new Error(response.body?.message || response.error || '获取下载音质失败，请重试')
      }
      return { status: 'ready', options: getDownloadQualityOptions(song, privilege) }
    } catch (error) {
      return {
        status: 'error',
        message: error instanceof Error ? error.message : '获取下载音质失败，请重试'
      }
    }
  },

  downloadSong(
    payload: SongDownloadRequest,
    parent: BrowserWindow | null
  ): Promise<SongDownloadResult> {
    const existing = activeDownloads.get(payload.songId)
    if (existing) return existing
    const download = saveSong(payload, parent).finally(() => activeDownloads.delete(payload.songId))
    activeDownloads.set(payload.songId, download)
    return download
  }
}
