import fs from 'fs'
import path from 'path'
import { protocol } from 'electron'
import { Readable } from 'stream'
import { isPathInsideRoot } from '../service/cacheService'

const CACHE_ASSET_SCHEME = 'ncm-cache'
const CACHE_ASSET_HOST = 'asset'

const MIME_TYPES: Record<string, string> = {
  '.mp3': 'audio/mpeg',
  '.flac': 'audio/flac',
  '.ogg': 'audio/ogg',
  '.m4a': 'audio/mp4',
  '.aac': 'audio/aac',
  '.wav': 'audio/wav',
  '.weba': 'audio/webm'
}

function getMimeType(filePath: string): string {
  const ext = path.extname(filePath).toLowerCase()
  return MIME_TYPES[ext] ?? 'application/octet-stream'
}

function isPathInsideCacheRoot(filePath: string, rootDir: string): boolean {
  if (!isPathInsideRoot(filePath, rootDir)) {
    return false
  }

  try {
    const realPath = fs.realpathSync(filePath)
    return isPathInsideRoot(realPath, rootDir)
  } catch {
    return false
  }
}

protocol.registerSchemesAsPrivileged([
  {
    scheme: CACHE_ASSET_SCHEME,
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      corsEnabled: true
    }
  }
])

export function createCacheAssetUrl(filePath: string): string {
  return `${CACHE_ASSET_SCHEME}://${CACHE_ASSET_HOST}?path=${encodeURIComponent(filePath)}`
}

export function registerCacheProtocol(cacheRootDir: string): void {
  const normalizedRootDir = path.resolve(cacheRootDir)

  protocol.handle(CACHE_ASSET_SCHEME, async (request) => {
    const requestUrl = new URL(request.url)

    if (requestUrl.hostname !== CACHE_ASSET_HOST) {
      console.warn(`[ncm-cache] invalid host: ${requestUrl.hostname}`)
      return new Response('Not Found', { status: 404 })
    }

    const requestedPath = requestUrl.searchParams.get('path')
    if (!requestedPath) {
      console.warn('[ncm-cache] missing path param')
      return new Response('Bad Request', { status: 400 })
    }

    const normalizedTargetPath = path.resolve(requestedPath)

    if (!isPathInsideCacheRoot(normalizedTargetPath, normalizedRootDir)) {
      console.warn(`[ncm-cache] outside cache root: ${normalizedTargetPath}`)
      return new Response('Forbidden', { status: 403 })
    }

    if (!fs.existsSync(normalizedTargetPath)) {
      console.warn(`[ncm-cache] file not found: ${normalizedTargetPath}`)
      return new Response('Not Found', { status: 404 })
    }

    const stat = await fs.promises.stat(normalizedTargetPath)
    if (!stat.isFile()) {
      console.warn(`[ncm-cache] not a file: ${normalizedTargetPath}`)
      return new Response('Not Found', { status: 404 })
    }

    const fileSize = stat.size
    const mimeType = getMimeType(normalizedTargetPath)
    const rangeHeader = request.headers.get('Range')

    if (!rangeHeader) {
      const stream = fs.createReadStream(normalizedTargetPath)
      return new Response(Readable.toWeb(stream) as ReadableStream<Uint8Array>, {
        status: 200,
        headers: {
          'Content-Type': mimeType,
          'Content-Length': String(fileSize),
          'Accept-Ranges': 'bytes'
        }
      })
    }

    const match = rangeHeader.match(/^bytes=(\d+)-(\d*)$/)
    if (!match) {
      return new Response('Bad Request', { status: 400 })
    }

    const start = parseInt(match[1], 10)
    const end = match[2] ? Math.min(parseInt(match[2], 10), fileSize - 1) : fileSize - 1

    if (start >= fileSize || start > end) {
      return new Response('Range Not Satisfiable', {
        status: 416,
        headers: { 'Content-Range': `bytes */${fileSize}` }
      })
    }

    const contentLength = end - start + 1
    const stream = fs.createReadStream(normalizedTargetPath, { start, end })
    return new Response(Readable.toWeb(stream) as ReadableStream<Uint8Array>, {
      status: 206,
      headers: {
        'Content-Type': mimeType,
        'Content-Length': String(contentLength),
        'Content-Range': `bytes ${start}-${end}/${fileSize}`,
        'Accept-Ranges': 'bytes'
      }
    })
  })
}
