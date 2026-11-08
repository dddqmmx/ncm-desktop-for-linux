'use strict'
// Versioned transport adapter. No Electron, playback, credentials storage or UI here.
// Keep stdout exclusively for protocol frames, including during module loading.
const write = process.stdout.write.bind(process.stdout)
if (require.main === module) for (const name of ['log', 'info', 'debug', 'warn', 'error']) console[name] = () => {}
const { createInterface } = require('node:readline')
const METHODS = Object.freeze({
  login: 'login_cellphone', captcha_sent: 'captcha_sent', banner: 'banner',
  user_cloud: 'user_cloud', search: 'cloudsearch', song_detail: 'song_detail',
  login_qr_key: 'login_qr_key', login_qr_create: 'login_qr_create', login_qr_check: 'login_qr_check',
  user_account: 'user_account', playlist_catlist: 'playlist_catlist', user_playlist: 'user_playlist',
  playlist_detail: 'playlist_detail', lyric: 'lyric_new', recommend_resource: 'recommend_resource',
  recommend_songs: 'recommend_songs', personalized: 'personalized', personalized_newsong: 'personalized_newsong',
  like: 'like', likelist: 'likelist', comment_music: 'comment_music',
  playlist_create: 'playlist_create', playlist_delete: 'playlist_delete',
  song_download_url: 'song_download_url_v1', song_download_detail: 'song_detail',
  artist_detail: 'artist_detail', artist_top_song: 'artist_top_song', artist_album: 'artist_album',
  artist_mv: 'artist_mv', album: 'album', logout: 'logout', mv_url: 'mv_url',
  album_newest: 'album_newest', toplist_detail: 'toplist_detail'
})
const UPDATE_PATHS = ['/api/playlist/update/name', '/api/playlist/desc/update']
function failure(error) {
  // Business rejections are often plain objects, not Error instances.
  if (error?.body && typeof error.body === 'object') {
    return { status: Number(error.status) || 502, body: error.body, cookie: [] }
  }
  return { status: 502, body: { code: 502, message: '网易云请求失败，请检查网络或 API 服务配置' }, cookie: [] }
}
async function dispatch(api, generateConfig, request) {
  try {
    if (request.version !== 1 || typeof request.operation !== 'string') throw new Error('protocol')
    const p = { ...(request.params || {}), cookie: request.cookie || '' }
    let result
    if (request.operation === 'ping') {
      result = { status: 200, body: { code: 200, provider: 'node', version: 1 } }
    } else if (request.operation === 'configure_xeapi') {
      await generateConfig()
      result = { status: 200, body: { code: 200 } }
    } else if (request.operation === 'song_url') {
      const missingKey = (e) => e instanceof Error && e.message.includes('xeapi public key')
      try { result = await api.song_url_v1(p) } catch (error) {
        if (!missingKey(error)) throw error
        await generateConfig().catch(() => {})
        try { result = await api.song_url_v1(p) } catch (retryError) {
          if (!missingKey(retryError)) throw retryError
          result = await api.song_url({ ...p, br: p.level === 'standard' ? 128000 : p.level === 'exhigh' ? 320000 : 999000 })
          if (Array.isArray(result.body?.data)) result.body.data = result.body.data.map(item => ({
            ...item, level: item.level || (item.br >= 999000 ? 'lossless' : item.br >= 320000 ? 'exhigh' : 'standard')
          }))
        }
      }
    } else if (request.operation === 'playlist_track_add' || request.operation === 'playlist_track_delete') {
      result = await api.playlist_tracks({ cookie: p.cookie, pid: p.pid,
        tracks: Array.isArray(p.tracks) ? p.tracks.join(',') : p.tracks,
        op: request.operation === 'playlist_track_add' ? 'add' : 'del' })
    } else if (request.operation === 'playlist_update') {
      result = await api.batch({ cookie: p.cookie,
        [UPDATE_PATHS[0]]: JSON.stringify({ id: p.id, name: p.name }),
        [UPDATE_PATHS[1]]: JSON.stringify({ id: p.id, desc: p.desc }) })
      if (result.status === 200 && (!result.body?.code || result.body.code === 200)) {
        for (const path of UPDATE_PATHS) {
          if (result.body?.[path]?.code !== 200) return {
            status: 200, body: { code: result.body?.[path]?.code || 502,
              message: result.body?.[path]?.message || result.body?.[path]?.msg || '歌单信息未完整保存，请刷新后重试' }, cookie: []
          }
        }
        result = { ...result, body: { code: 200 } }
      }
    } else {
      const method = METHODS[request.operation]
      if (!method || typeof api[method] !== 'function') return {
        status: 400, body: { code: 400, message: '不支持的 API 操作' }, cookie: []
      }
      result = await api[method](p)
    }
    return { status: result.status || 200, body: result.body ?? null, cookie: result.cookie || [] }
  } catch (error) { return failure(error) }
}
if (require.main === module) {
  let api, config
  try {
    api = require('@neteasecloudmusicapienhanced/api')
    config = require('@neteasecloudmusicapienhanced/api/generateConfig')
  } catch {
    process.stderr.write('NCM API dependency missing: run npm install in the project directory.\n')
    process.exit(1)
  }
  let active = 0, ended = false
  const lines = createInterface({ input: process.stdin, crlfDelay: Infinity })
  lines.on('line', async line => {
    if (Buffer.byteLength(line) > 1024 * 1024 || active >= 64) { process.exitCode = 1; lines.close(); process.stdin.destroy(); return }
    active++
    let request
    try {
      request = JSON.parse(line)
      const response = await dispatch(api, config, request)
      write(JSON.stringify({ version: 1, id: request.id, ...response }) + '\n')
    } catch { write(JSON.stringify({ version: 1, id: request?.id, ...failure(null) }) + '\n') }
    finally { active--; if (ended && active === 0) process.exit(0) }
  })
  lines.on('close', () => { ended = true; if (active === 0) process.exit(0) })
}
module.exports = { dispatch, METHODS }
