import fs from 'fs'
import path from 'path'
import { net, protocol } from 'electron'
import { pathToFileURL } from 'url'

const CACHE_ASSET_SCHEME = 'ncm-cache'
const CACHE_ASSET_HOST = 'asset'

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
      return new Response('Not Found', { status: 404 })
    }

    const requestedPath = requestUrl.searchParams.get('path')
    if (!requestedPath) {
      return new Response('Bad Request', { status: 400 })
    }

    const normalizedTargetPath = path.resolve(requestedPath)
    const isInsideCacheRoot =
      normalizedTargetPath === normalizedRootDir ||
      normalizedTargetPath.startsWith(`${normalizedRootDir}${path.sep}`)

    if (!isInsideCacheRoot) {
      return new Response('Forbidden', { status: 403 })
    }

    if (!fs.existsSync(normalizedTargetPath)) {
      return new Response('Not Found', { status: 404 })
    }

    return net.fetch(pathToFileURL(normalizedTargetPath).toString())
  })
}
