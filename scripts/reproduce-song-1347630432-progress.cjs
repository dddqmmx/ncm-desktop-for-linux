#!/usr/bin/env node

const path = require('node:path')
const { setTimeout: sleep } = require('node:timers/promises')
const fs = require('node:fs')
const os = require('node:os')
const { lyric_new, song_detail, song_url_v1 } = require('@neteasecloudmusicapienhanced/api')
const { CacheService, PlayerService } = require('../native/index.node')

const SONG_ID = Number(process.env.SONG_ID ?? 1347630432)
const START_SECS = Number(process.env.START_SECS ?? 0)
const SAMPLE_MS = Number(process.env.SAMPLE_MS ?? 100)
const DURATION_SECS = Number(process.env.DURATION_SECS ?? 70)
const QUALITY = process.env.QUALITY ?? 'standard'
const MODE = process.env.MODE ?? 'split'
const EXPECT_FINISH = process.env.EXPECT_FINISH === '1'
const OUTPUT_DEVICE_ID = process.env.OUTPUT_DEVICE_ID?.trim() || ''
const JUMP_THRESHOLD_MS = Number(process.env.JUMP_THRESHOLD_MS ?? 900)
const STALL_THRESHOLD_MS = Number(process.env.STALL_THRESHOLD_MS ?? 10_000)
const END_TOLERANCE_MS = Number(process.env.END_TOLERANCE_MS ?? 2_500)
const LYRIC_OFFSET_MS = 400
const LOCAL_STORAGE_DIRS = [
  process.env.ELECTRON_LOCAL_STORAGE_DIR,
  path.join(os.homedir(), '.config', 'ncm-desktop-for-linux', 'Local Storage', 'leveldb'),
  path.join(os.homedir(), '.config', 'Electron', 'Local Storage', 'leveldb')
].filter(Boolean)

function parseTimedLines(lrcString) {
  const timeExp = /\[(\d{2}):(\d{2})\.(\d{2,3})\]/
  return String(lrcString ?? '')
    .split('\n')
    .flatMap((line) => {
      const match = timeExp.exec(line)
      if (!match) return []

      const minutes = Number(match[1])
      const seconds = Number(match[2])
      const fraction = Number(match[3])
      const time =
        (minutes * 60 + seconds) * 1000 + (match[3].length === 3 ? fraction : fraction * 10)
      const text = line.replace(timeExp, '').trim()
      return text ? [{ time, text }] : []
    })
    .sort((a, b) => a.time - b.time)
}

function findActiveLyricIndex(lines, progressMs) {
  const adjusted = progressMs + LYRIC_OFFSET_MS
  for (let index = lines.length - 1; index >= 0; index--) {
    if (adjusted >= lines[index].time) return index
  }
  return -1
}

async function callApi(name, fn, params) {
  const result = await fn(params)
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${name} failed with status ${result.status}`)
  }
  return result.body
}

function readCookieFromEnvOrFile() {
  if (process.env.COOKIE) return process.env.COOKIE.trim()
  if (!process.env.COOKIE_FILE) return ''

  try {
    return fs.readFileSync(process.env.COOKIE_FILE, 'utf8').trim()
  } catch (error) {
    throw new Error(`Failed to read COOKIE_FILE: ${error.message}`)
  }
}

function isCookieByte(byte) {
  return byte >= 0x20 && byte <= 0x7e
}

function extractCookieAround(buffer, markerIndex) {
  let start = markerIndex
  while (start > 0 && isCookieByte(buffer[start - 1])) start--

  let end = markerIndex
  while (end < buffer.length && isCookieByte(buffer[end])) end++

  const value = buffer.toString('utf8', start, end).trim()
  const musicUserAt = value.indexOf('MUSIC_U=')
  if (musicUserAt < 0) return ''

  const cookie = value.slice(musicUserAt)
  const nextRecordAt = cookie.search(/(?:\\x00|_https?:|file:\/\/|app_cookie|currentSong)/)
  return (nextRecordAt > 0 ? cookie.slice(0, nextRecordAt) : cookie).trim()
}

function extractCookieFromLevelDbFile(filePath) {
  const buffer = fs.readFileSync(filePath)
  const marker = Buffer.from('MUSIC_U=', 'utf8')
  let offset = 0

  while (offset < buffer.length) {
    const markerIndex = buffer.indexOf(marker, offset)
    if (markerIndex < 0) return ''

    const cookie = extractCookieAround(buffer, markerIndex)
    if (cookie.includes('MUSIC_U=')) return cookie
    offset = markerIndex + marker.length
  }

  return ''
}

function readCookieFromElectronLocalStorage() {
  for (const dir of LOCAL_STORAGE_DIRS) {
    if (!dir || !fs.existsSync(dir)) continue

    const files = fs
      .readdirSync(dir)
      .filter((name) => /\.(?:log|ldb|sst)$/i.test(name))
      .map((name) => path.join(dir, name))
      .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs)

    for (const filePath of files) {
      const cookie = extractCookieFromLevelDbFile(filePath)
      if (cookie) {
        return { cookie, source: dir }
      }
    }
  }

  return { cookie: '', source: '' }
}

function resolveCookie() {
  const explicitCookie = readCookieFromEnvOrFile()
  if (explicitCookie) {
    console.log(`Cookie source: explicit env/file (${explicitCookie.length} bytes)`)
    return explicitCookie
  }

  if (process.env.AUTO_ELECTRON_COOKIE === '0') return ''

  const { cookie, source } = readCookieFromElectronLocalStorage()
  if (cookie) {
    console.log(`Cookie source: Electron localStorage (${cookie.length} bytes, ${source})`)
  }
  return cookie
}

async function main() {
  console.log(
    `Reproducing song ${SONG_ID} from ${START_SECS}s for ${DURATION_SECS}s ` +
      `(mode=${MODE}, quality=${QUALITY}, expectFinish=${EXPECT_FINISH})`
  )

  const cookie = resolveCookie()
  const [detailBody, lyricBody, urlBody] = await Promise.all([
    callApi('song_detail', song_detail, { ids: String(SONG_ID), cookie }),
    callApi('lyric_new', lyric_new, { id: SONG_ID, cookie }),
    callApi('song_url_v1', song_url_v1, { id: SONG_ID, level: QUALITY, cookie })
  ])

  const song = detailBody?.songs?.[0]
  const urlData = urlBody?.data?.[0]
  const songUrl = urlData?.url
  if (!songUrl) {
    throw new Error(`No playable URL returned for song ${SONG_ID}`)
  }

  const returnedDuration = Number(urlData?.time ?? 0)
  const songDuration = Number(song?.dt ?? 0)
  if (urlData?.freeTrialInfo || (songDuration > 0 && returnedDuration < songDuration - 10_000)) {
    throw new Error(
      `API returned a trial/short URL (${returnedDuration}ms of ${songDuration}ms). ` +
        'Run with COOKIE="MUSIC_U=...; ..." from a logged-in account that can play the full song.'
    )
  }

  const lyrics = parseTimedLines(lyricBody?.lrc?.lyric)
  console.log(`Song: ${song?.name ?? '(unknown)'} (${song?.dt ?? 0}ms)`)
  console.log(`URL level: ${urlData?.level ?? QUALITY}; URL duration: ${returnedDuration}ms`)
  console.log(`Lyric lines: ${lyrics.length}`)

  const player = new PlayerService()
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), `ncm-song-${SONG_ID}-`))
  const cachePath = path.join(tempDir, `${SONG_ID}.audio`)
  const metadataPath = path.join(tempDir, `${SONG_ID}.json`)
  let backgroundCache = null
  let backgroundCacheMetadataPath = ''
  let backgroundCacheProgress = null
  let backgroundCacheError = ''

  if (MODE === 'cached') {
    fs.writeFileSync(cachePath, '')
    fs.writeFileSync(
      metadataPath,
      JSON.stringify({
        song_id: SONG_ID,
        quality: QUALITY,
        source_url: songUrl,
        content_length: urlBody?.data?.[0]?.size ?? null,
        downloaded_ranges: [],
        is_complete: false,
        updated_at: Math.floor(Date.now() / 1000)
      })
    )
  } else if (MODE === 'split') {
    backgroundCache = new CacheService(path.join(tempDir, 'cache'), 512 * 1024 * 1024)
    await backgroundCache.setSongMaxCacheAheadBytes(16 * 1024 * 1024)
    const prepared = await backgroundCache.prepareSongSource(
      SONG_ID,
      QUALITY,
      songUrl,
      urlData?.size
    )
    backgroundCacheMetadataPath = prepared.metadataPath ?? ''
    void backgroundCache
      .cacheSongSource(SONG_ID, QUALITY, songUrl, urlData?.size, songDuration)
      .catch((error) => {
        backgroundCacheError = error instanceof Error ? error.message : String(error)
      })
    console.log(`Background cache metadata: ${backgroundCacheMetadataPath}`)
  }
  const samples = []
  const anomalies = []
  const stalls = []
  let lastProgress = null
  let lastWallMs = null
  let lastAdvanceWallMs = 0
  let stallReported = false
  let endedNaturally = false
  let maxProgress = 0

  try {
    if (OUTPUT_DEVICE_ID) {
      await player.switchOutputDevice(OUTPUT_DEVICE_ID)
      console.log(`Output device: ${OUTPUT_DEVICE_ID}`)
    }

    if (MODE === 'cached') {
      player.playUrlCached(
        songUrl,
        cachePath,
        metadataPath,
        song?.dt,
        30,
        16 * 1024 * 1024,
        START_SECS
      )
    } else {
      player.playUrl(songUrl, START_SECS)
    }
    await sleep(600)

    const startedAt = Date.now()
    while (Date.now() - startedAt <= DURATION_SECS * 1000) {
      const wallMs = Date.now() - startedAt
      const progress = player.progressMs
      if (backgroundCache && backgroundCacheMetadataPath) {
        backgroundCache.updateSongCachePlaybackPosition(backgroundCacheMetadataPath, progress)
      }
      const isPlaying = player.isPlaying
      const isBuffering = player.isBuffering
      const lyricIndex = findActiveLyricIndex(lyrics, progress)
      const lyric = lyricIndex >= 0 ? lyrics[lyricIndex] : undefined
      const sample = {
        wallMs,
        progress,
        isPlaying,
        isBuffering,
        lyricIndex,
        lyricTime: lyric?.time,
        lyricText: lyric?.text ?? ''
      }

      if (lastProgress !== null && lastWallMs !== null) {
        const progressDelta = progress - lastProgress
        const wallDelta = wallMs - lastWallMs
        if (progressDelta > 0) {
          lastAdvanceWallMs = wallMs
          stallReported = false
        } else if (
          isPlaying &&
          wallMs - lastAdvanceWallMs >= STALL_THRESHOLD_MS &&
          !stallReported
        ) {
          const stall = {
            atWallMs: wallMs,
            progress,
            stalledForMs: wallMs - lastAdvanceWallMs,
            isBuffering
          }
          stalls.push(stall)
          stallReported = true
          console.log(
            `STALL wall=${wallMs}ms progress=${progress}ms ` +
              `stalledFor=${stall.stalledForMs}ms buffering=${isBuffering}`
          )
        }
        if (progressDelta < -200 || progressDelta - wallDelta > JUMP_THRESHOLD_MS) {
          const anomaly = {
            atWallMs: wallMs,
            fromProgress: lastProgress,
            toProgress: progress,
            progressDelta,
            wallDelta,
            lyricIndex,
            lyricText: sample.lyricText
          }
          anomalies.push(anomaly)
          console.log(
            `ANOMALY wall=${wallMs}ms progress ${lastProgress}->${progress} ` +
              `delta=${progressDelta}ms lyric#${lyricIndex} ${JSON.stringify(sample.lyricText)}`
          )
        }
      }

      if (progress >= 57_500 && progress <= 61_500) {
        console.log(
          `TRACE wall=${wallMs}ms progress=${progress}ms ` +
            `lyric#${lyricIndex}@${lyric?.time ?? '-'} ${JSON.stringify(sample.lyricText)}`
        )
      }

      samples.push(sample)
      maxProgress = Math.max(maxProgress, progress)
      lastProgress = progress
      lastWallMs = wallMs

      if (
        !isPlaying &&
        songDuration > 0 &&
        progress >= Math.max(0, songDuration - END_TOLERANCE_MS)
      ) {
        endedNaturally = true
        console.log(`ENDED wall=${wallMs}ms progress=${progress}ms`)
        break
      }
      await sleep(SAMPLE_MS)
    }

    if (maxProgress < Math.max(1_000, START_SECS * 1000 + 1_000)) {
      throw new Error(`Playback did not start; max observed progress was ${maxProgress}ms`)
    }
  } finally {
    if (backgroundCache && backgroundCacheMetadataPath) {
      try {
        backgroundCache.updateSongCachePlaybackPosition(
          backgroundCacheMetadataPath,
          endedNaturally ? songDuration : maxProgress
        )
        await sleep(250)
        backgroundCacheProgress = await backgroundCache.getSongCacheProgress(
          backgroundCacheMetadataPath
        )
        await backgroundCache.cancelSongCacheDownload(backgroundCacheMetadataPath)
      } catch (error) {
        backgroundCacheError = error instanceof Error ? error.message : String(error)
      }
    }
    try {
      player.stop()
    } catch {
      // ignore cleanup failures
    }
  }

  const reportPath = path.resolve(process.cwd(), `song-${SONG_ID}-progress-report.json`)
  require('node:fs').writeFileSync(
    reportPath,
    JSON.stringify(
      {
        songId: SONG_ID,
        startSecs: START_SECS,
        sampleMs: SAMPLE_MS,
        durationSecs: DURATION_SECS,
        quality: QUALITY,
        expectedSongDurationMs: songDuration,
        endedNaturally,
        maxProgress,
        backgroundCacheProgress,
        backgroundCacheError,
        anomalies,
        stalls,
        samples
      },
      null,
      2
    )
  )

  console.log(`Report: ${reportPath}`)
  console.log(`Temp cache: ${tempDir}`)
  console.log(
    `Result: endedNaturally=${endedNaturally}, maxProgress=${maxProgress}ms, ` +
      `stalls=${stalls.length}, anomalies=${anomalies.length}`
  )
  if (backgroundCacheProgress) {
    console.log(`Background cache: ${JSON.stringify(backgroundCacheProgress)}`)
  }
  if (backgroundCacheError) {
    console.error(`Background cache error: ${backgroundCacheError}`)
  }
  if (EXPECT_FINISH && !endedNaturally) {
    console.error(`Playback did not finish naturally within ${DURATION_SECS}s`)
    process.exitCode = 1
  } else if (stalls.length > 0 || anomalies.length > 0) {
    process.exitCode = 2
  }
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
