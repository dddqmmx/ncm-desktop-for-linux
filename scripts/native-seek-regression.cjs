#!/usr/bin/env node

const fs = require('node:fs')
const http = require('node:http')
const os = require('node:os')
const path = require('node:path')
const { performance } = require('node:perf_hooks')
const { spawn, spawnSync } = require('node:child_process')
const { setTimeout: sleep } = require('node:timers/promises')

const ROOT = path.resolve(__dirname, '..')
const NATIVE_DIR = path.join(ROOT, 'native')
const DEFAULT_REPORT_PATH = path.join(ROOT, 'native-seek-regression-report.json')

const args = new Set(process.argv.slice(2))
const runCargo = !args.has('--skip-cargo')
const runAudio = !args.has('--skip-audio')
const buildNative = !args.has('--no-build-native')
const keepTemp = args.has('--keep-temp')
const captureMonitor = args.has('--capture-monitor')
const captureDeviceArg = process.argv.find((arg) => arg.startsWith('--capture-device='))
const captureDevice = captureDeviceArg?.slice('--capture-device='.length) ?? process.env.SEEK_TEST_CAPTURE_DEVICE
const runCachedUrlProbe = !args.has('--skip-cached-url')
const audioFormatArg = process.argv.find((arg) => arg.startsWith('--audio-format='))
const audioFormat = audioFormatArg?.slice('--audio-format='.length) ?? process.env.SEEK_TEST_AUDIO_FORMAT ?? 'all'
const requireCompressed = args.has('--require-compressed') || captureMonitor
const reportArg = process.argv.find((arg) => arg.startsWith('--report='))
const reportPath = path.resolve(reportArg ? reportArg.slice('--report='.length) : DEFAULT_REPORT_PATH)

const SAMPLE_RATE = Number(process.env.SEEK_TEST_SAMPLE_RATE ?? 48_000)
const CHANNELS = 2
const DURATION_SECS = Number(process.env.SEEK_TEST_DURATION_SECS ?? 55)
const SAMPLE_MS = Number(process.env.SEEK_TEST_SAMPLE_MS ?? 25)
const TONE_AMPLITUDE = Number(process.env.SEEK_TEST_TONE_AMPLITUDE ?? (captureMonitor ? 0.05 : 0))
const CONTENT_WINDOW_MS = Number(process.env.SEEK_TEST_CONTENT_WINDOW_MS ?? 260)
const CONTENT_SECOND_TOLERANCE = Number(process.env.SEEK_TEST_CONTENT_SECOND_TOLERANCE ?? 1)
const CONTENT_MIN_POWER = Number(process.env.SEEK_TEST_CONTENT_MIN_POWER ?? 1e7)
const CONTENT_MAX_MISMATCH_RATIO = Number(process.env.SEEK_TEST_CONTENT_MAX_MISMATCH_RATIO ?? 0.2)
const CAPTURE_CALIBRATION_TIMEOUT_MS = Number(process.env.SEEK_TEST_CAPTURE_CALIBRATION_TIMEOUT_MS ?? 5_000)
const CAPTURE_MAX_LAG_MS = Number(process.env.SEEK_TEST_CAPTURE_MAX_LAG_MS ?? 4_000)
const CAPTURE_SETTLE_EXTRA_MS = Number(process.env.SEEK_TEST_CAPTURE_SETTLE_EXTRA_MS ?? 350)
const START_AT_RESTORE_MS = Number(process.env.SEEK_TEST_START_AT_RESTORE_MS ?? 12_500)
const CACHED_URL_MAX_WRITE_AHEAD_BYTES = Number(
  process.env.SEEK_TEST_CACHED_URL_MAX_WRITE_AHEAD_BYTES ?? 64 * 1024
)
const NATIVE_CALL_TIMEOUT_MS = Number(process.env.SEEK_TEST_NATIVE_CALL_TIMEOUT_MS ?? 8_000)

const SEEK_ANCHOR_TOLERANCE_MS = Number(process.env.SEEK_TEST_ANCHOR_TOLERANCE_MS ?? 450)
const SEEK_ANCHOR_TIMEOUT_MS = Number(process.env.SEEK_TEST_ANCHOR_TIMEOUT_MS ?? 1_500)
const BUFFER_TIMEOUT_MS = Number(process.env.SEEK_TEST_BUFFER_TIMEOUT_MS ?? 3_000)
const STEADY_WINDOW_MS = Number(process.env.SEEK_TEST_STEADY_WINDOW_MS ?? (captureMonitor ? 1_200 : 700))
const STEADY_RATE_MIN = Number(process.env.SEEK_TEST_RATE_MIN ?? 0.45)
const STEADY_RATE_MAX = Number(process.env.SEEK_TEST_RATE_MAX ?? 1.85)
const MAX_STEADY_BACKWARD_MS = Number(process.env.SEEK_TEST_MAX_BACKWARD_MS ?? 120)
const MAX_STEADY_STEP_EXCESS_MS = Number(process.env.SEEK_TEST_MAX_STEP_EXCESS_MS ?? 550)

const report = {
  config: {
    sampleRate: SAMPLE_RATE,
    channels: CHANNELS,
    durationSecs: DURATION_SECS,
    sampleMs: SAMPLE_MS,
    seekAnchorToleranceMs: SEEK_ANCHOR_TOLERANCE_MS,
    seekAnchorTimeoutMs: SEEK_ANCHOR_TIMEOUT_MS,
    bufferTimeoutMs: BUFFER_TIMEOUT_MS,
    steadyWindowMs: STEADY_WINDOW_MS,
    steadyRateMin: STEADY_RATE_MIN,
    steadyRateMax: STEADY_RATE_MAX,
    maxSteadyBackwardMs: MAX_STEADY_BACKWARD_MS,
    maxSteadyStepExcessMs: MAX_STEADY_STEP_EXCESS_MS,
    toneAmplitude: TONE_AMPLITUDE,
    captureMonitor,
    captureDevice: captureDevice ?? null,
    audioFormat,
    requireCompressed,
    contentWindowMs: CONTENT_WINDOW_MS,
    contentSecondTolerance: CONTENT_SECOND_TOLERANCE,
    contentMinPower: CONTENT_MIN_POWER,
    contentMaxMismatchRatio: CONTENT_MAX_MISMATCH_RATIO,
    captureCalibrationTimeoutMs: CAPTURE_CALIBRATION_TIMEOUT_MS,
    captureMaxLagMs: CAPTURE_MAX_LAG_MS,
    captureSettleExtraMs: CAPTURE_SETTLE_EXTRA_MS,
    startAtRestoreMs: START_AT_RESTORE_MS,
    nativeCallTimeoutMs: NATIVE_CALL_TIMEOUT_MS,
    runCachedUrlProbe,
    cachedUrlMaxWriteAheadBytes: CACHED_URL_MAX_WRITE_AHEAD_BYTES
  },
  commands: [],
  checks: [],
  samples: [],
  failures: []
}

function usage() {
  console.log(`Usage: node scripts/native-seek-regression.cjs [options]

Options:
  --skip-cargo       Skip cargo test regression coverage.
  --skip-audio       Skip the real NAPI PlayerService playback probe.
  --no-build-native  Do not rebuild native/index.node before the NAPI probe.
  --keep-temp        Keep the generated WAV temp directory.
  --capture-monitor  Record the default PulseAudio/PipeWire monitor and compare actual content.
  --capture-device=<source>
                     Override capture source, e.g. alsa_output....monitor.
  --audio-format=<all|wav|vorbis|mp3|m4a|flac>
                     Probe WAV and/or local compressed transcodes. Default: all.
  --require-compressed
                     Fail if the compressed local seek probe cannot be generated.
  --skip-cached-url  Skip the playUrlCached(start_at) probe.
  --report=<path>    Write the JSON report to a custom path.

Environment:
  SEEK_TEST_TONE_AMPLITUDE=0.02 overrides the generated test tone amplitude.
  SEEK_TEST_* thresholds can tune timing for slow output devices.
`)
}

function fail(message, details = {}) {
  const failure = { message, ...details }
  report.failures.push(failure)
  throw Object.assign(new Error(message), { details })
}

function recordCommand(command, result) {
  report.commands.push({
    command,
    status: result.status,
    signal: result.signal ?? null
  })
}

function runCommand(command, commandArgs, options = {}) {
  console.log(`$ ${[command, ...commandArgs].join(' ')}`)
  const result = spawnSync(command, commandArgs, {
    cwd: ROOT,
    stdio: 'inherit',
    shell: false,
    ...options
  })
  recordCommand([command, ...commandArgs], result)
  if (result.error) {
    fail(`Command failed to start: ${command}`, { error: result.error.message })
  }
  if (result.status !== 0) {
    fail(`Command failed: ${[command, ...commandArgs].join(' ')}`, {
      status: result.status,
      signal: result.signal
    })
  }
}

function runCommandCapture(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: ROOT,
    encoding: 'utf8',
    shell: false,
    ...options
  })
  recordCommand([command, ...commandArgs], result)
  if (result.error) return { ok: false, error: result.error.message, stdout: '', stderr: '' }
  return {
    ok: result.status === 0,
    status: result.status,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? ''
  }
}

function commandExists(command) {
  const result = spawnSync('which', [command], { encoding: 'utf8' })
  return result.status === 0
}

function frequencyForSecond(second) {
  return 180 + second * 11
}

function writeTestWav(filePath) {
  const bytesPerSample = 2
  const totalFrames = Math.floor(SAMPLE_RATE * DURATION_SECS)
  const dataBytes = totalFrames * CHANNELS * bytesPerSample
  const buffer = Buffer.alloc(44 + dataBytes)

  buffer.write('RIFF', 0)
  buffer.writeUInt32LE(36 + dataBytes, 4)
  buffer.write('WAVE', 8)
  buffer.write('fmt ', 12)
  buffer.writeUInt32LE(16, 16)
  buffer.writeUInt16LE(1, 20)
  buffer.writeUInt16LE(CHANNELS, 22)
  buffer.writeUInt32LE(SAMPLE_RATE, 24)
  buffer.writeUInt32LE(SAMPLE_RATE * CHANNELS * bytesPerSample, 28)
  buffer.writeUInt16LE(CHANNELS * bytesPerSample, 32)
  buffer.writeUInt16LE(16, 34)
  buffer.write('data', 36)
  buffer.writeUInt32LE(dataBytes, 40)

  let offset = 44
  for (let frame = 0; frame < totalFrames; frame++) {
    const second = Math.floor(frame / SAMPLE_RATE)
    const frequency = frequencyForSecond(second)
    const value =
      TONE_AMPLITUDE === 0
        ? 0
        : Math.round(Math.sin((2 * Math.PI * frequency * frame) / SAMPLE_RATE) * 32767 * TONE_AMPLITUDE)

    for (let channel = 0; channel < CHANNELS; channel++) {
      buffer.writeInt16LE(value, offset)
      offset += bytesPerSample
    }
  }

  fs.writeFileSync(filePath, buffer)
}

function transcodeAudio(wavPath, outputPath, ffmpegArgs) {
  if (!commandExists('ffmpeg')) {
    return { ok: false, reason: 'ffmpeg is not installed' }
  }

  const command = [
    'ffmpeg',
    '-y',
    '-hide_banner',
    '-loglevel',
    'error',
    '-i',
    wavPath,
    '-map',
    '0:a:0',
    ...ffmpegArgs,
    outputPath
  ]
  console.log(`$ ${command.join(' ')}`)
  const result = spawnSync(command[0], command.slice(1), {
    cwd: ROOT,
    encoding: 'utf8',
    shell: false
  })
  recordCommand(command, result)
  if (result.error) {
    return { ok: false, reason: result.error.message }
  }
  if (result.status !== 0) {
    return {
      ok: false,
      reason: result.stderr?.trim() || `ffmpeg exited with ${result.status}`
    }
  }

  return { ok: true }
}

function buildAudioCases(wavPath, tempDir) {
  const compressedFormats = [
    {
      label: 'ogg-vorbis',
      format: 'vorbis',
      fileName: 'seek-regression.ogg',
      ffmpegArgs: ['-c:a', 'libvorbis', '-q:a', '5']
    },
    {
      label: 'mp3',
      format: 'mp3',
      fileName: 'seek-regression.mp3',
      ffmpegArgs: ['-c:a', 'libmp3lame', '-b:a', '192k']
    },
    {
      label: 'm4a-aac',
      format: 'm4a',
      fileName: 'seek-regression.m4a',
      ffmpegArgs: ['-c:a', 'aac', '-b:a', '192k']
    },
    {
      label: 'flac',
      format: 'flac',
      fileName: 'seek-regression.flac',
      ffmpegArgs: ['-c:a', 'flac']
    }
  ]

  const validFormats = new Set(['all', 'wav', ...compressedFormats.map((format) => format.format)])
  if (!validFormats.has(audioFormat)) {
    fail('Invalid --audio-format value', { audioFormat })
  }

  const cases = []
  if (audioFormat === 'all' || audioFormat === 'wav') {
    cases.push({ label: 'wav-pcm', path: wavPath, compressed: false })
  }

  for (const format of compressedFormats) {
    if (audioFormat !== 'all' && audioFormat !== format.format) {
      continue
    }

    const outputPath = path.join(tempDir, format.fileName)
    const result = transcodeAudio(wavPath, outputPath, format.ffmpegArgs)
    if (result.ok) {
      cases.push({ label: format.label, path: outputPath, compressed: true })
      report.checks.push({
        description: `generated ${format.label} local seek fixture`,
        ok: true,
        detail: { path: outputPath, codec: format.format }
      })
    } else if (requireCompressed || audioFormat === format.format) {
      fail(`Unable to generate ${format.label} local seek fixture`, result)
    } else {
      report.checks.push({
        description: `generated ${format.label} local seek fixture`,
        ok: false,
        detail: result
      })
    }
  }

  if (cases.length === 0) {
    fail('No audio probe cases selected', { audioFormat })
  }

  return cases
}

function parseRangeHeader(rangeHeader, size) {
  const match = /^bytes=(\d*)-(\d*)$/.exec(rangeHeader ?? '')
  if (!match) return null

  const startText = match[1]
  const endText = match[2]
  if (!startText && !endText) return null

  let start
  let end
  if (!startText) {
    const suffixLength = Number(endText)
    if (!Number.isFinite(suffixLength) || suffixLength <= 0) return null
    start = Math.max(0, size - suffixLength)
    end = size - 1
  } else {
    start = Number(startText)
    end = endText ? Number(endText) : size - 1
  }

  if (!Number.isFinite(start) || !Number.isFinite(end) || start > end || start >= size) {
    return null
  }

  return {
    start,
    end: Math.min(end, size - 1)
  }
}

function startRangeServer(filePath) {
  const stat = fs.statSync(filePath)
  const size = stat.size
  const server = http.createServer((req, res) => {
    const range = parseRangeHeader(req.headers.range, size)
    const headers = {
      'Accept-Ranges': 'bytes',
      'Content-Type': 'application/octet-stream'
    }

    if (range) {
      res.writeHead(206, {
        ...headers,
        'Content-Length': range.end - range.start + 1,
        'Content-Range': `bytes ${range.start}-${range.end}/${size}`
      })
      if (req.method === 'HEAD') {
        res.end()
        return
      }
      fs.createReadStream(filePath, range).pipe(res)
      return
    }

    res.writeHead(200, {
      ...headers,
      'Content-Length': size
    })
    if (req.method === 'HEAD') {
      res.end()
      return
    }
    fs.createReadStream(filePath).pipe(res)
  })

  return new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (!address || typeof address === 'string') {
        server.close()
        reject(new Error('Unable to start range server'))
        return
      }

      resolve({
        url: `http://127.0.0.1:${address.port}/${path.basename(filePath)}`,
        close: () => new Promise((closeResolve) => server.close(closeResolve))
      })
    })
  })
}

function findDefaultMonitorSource() {
  if (captureDevice) return captureDevice

  if (!commandExists('pactl')) {
    fail('--capture-monitor requires pactl or --capture-device=<source>')
  }

  const sink = runCommandCapture('pactl', ['get-default-sink'])
  if (!sink.ok) {
    fail('Unable to read default PulseAudio/PipeWire sink', {
      stderr: sink.stderr.trim()
    })
  }

  const defaultSink = sink.stdout.trim()
  if (!defaultSink) {
    fail('Default PulseAudio/PipeWire sink is empty')
  }

  return `${defaultSink}.monitor`
}

function goertzelPower(samples, frequency) {
  const omega = (2 * Math.PI * frequency) / SAMPLE_RATE
  const coeff = 2 * Math.cos(omega)
  let q0 = 0
  let q1 = 0
  let q2 = 0

  for (const sample of samples) {
    q0 = coeff * q1 - q2 + sample
    q2 = q1
    q1 = q0
  }

  return q1 * q1 + q2 * q2 - coeff * q1 * q2
}

class MonitorCapture {
  constructor(device) {
    this.device = device
    this.maxBytes = SAMPLE_RATE * CHANNELS * 2 * 6
    this.buffer = Buffer.alloc(0)
    this.child = null
  }

  start() {
    if (!commandExists('parecord')) {
      fail('--capture-monitor requires parecord from PulseAudio/PipeWire tools')
    }

    this.child = spawn(
      'parecord',
      [
        `--device=${this.device}`,
        '--format=s16le',
        `--rate=${SAMPLE_RATE}`,
        `--channels=${CHANNELS}`,
        '--latency-msec=20',
        '--process-time-msec=10',
        '--raw'
      ],
      {
        stdio: ['ignore', 'pipe', 'pipe']
      }
    )

    this.child.stdout.on('data', (chunk) => {
      this.buffer = Buffer.concat([this.buffer, chunk])
      if (this.buffer.length > this.maxBytes) {
        this.buffer = this.buffer.subarray(this.buffer.length - this.maxBytes)
      }
    })

    this.child.stderr.on('data', (chunk) => {
      const message = chunk.toString().trim()
      if (message) {
        report.checks.push({
          description: 'capture stderr',
          ok: true,
          detail: message
        })
      }
    })

    this.child.on('exit', (code, signal) => {
      if (code !== 0 && code !== null) {
        report.failures.push({
          message: 'parecord exited before the probe finished',
          code,
          signal
        })
      }
    })

    report.capture = {
      command: [
        'parecord',
        `--device=${this.device}`,
        '--format=s16le',
        `--rate=${SAMPLE_RATE}`,
        `--channels=${CHANNELS}`,
        '--latency-msec=20',
        '--process-time-msec=10',
        '--raw'
      ],
      device: this.device
    }
  }

  stop() {
    if (this.child && !this.child.killed) {
      this.child.kill('SIGTERM')
    }
  }

  reset() {
    this.buffer = Buffer.alloc(0)
  }

  estimateLatestSecond() {
    const frameBytes = CHANNELS * 2
    const windowFrames = Math.floor((SAMPLE_RATE * CONTENT_WINDOW_MS) / 1000)
    const windowBytes = windowFrames * frameBytes
    if (this.buffer.length < windowBytes) return null

    const start = this.buffer.length - windowBytes
    const samples = new Array(windowFrames)
    for (let i = 0; i < windowFrames; i++) {
      let sum = 0
      const frameOffset = start + i * frameBytes
      for (let channel = 0; channel < CHANNELS; channel++) {
        sum += this.buffer.readInt16LE(frameOffset + channel * 2)
      }
      samples[i] = sum / CHANNELS
    }

    let bestSecond = -1
    let bestFrequency = 0
    let bestPower = 0
    for (let second = 0; second < DURATION_SECS; second++) {
      const frequency = frequencyForSecond(second)
      const power = goertzelPower(samples, frequency)
      if (power > bestPower) {
        bestPower = power
        bestFrequency = frequency
        bestSecond = second
      }
    }

    return {
      second: bestSecond,
      frequency: bestFrequency,
      power: bestPower,
      ok: bestPower >= CONTENT_MIN_POWER
    }
  }
}

let captureProbe = null
let captureLagMs = 0

function median(values) {
  const sorted = [...values].sort((a, b) => a - b)
  const middle = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle]
}

function sample(player, phase) {
  const progressMs = player.progressMs
  const entry = {
    atMs: Math.round(performance.now()),
    phase,
    progressMs,
    isBuffering: player.isBuffering
  }

  if (captureProbe) {
    const content = captureProbe.estimateLatestSecond()
    if (content) {
      const expectedSecond = Math.max(
        0,
        Math.min(DURATION_SECS - 1, Math.floor(Math.max(0, progressMs - captureLagMs) / 1000))
      )
      entry.content = {
        ...content,
        expectedSecond,
        captureLagMs,
        secondDelta: content.second - expectedSecond,
        matchesProgress:
          content.ok && Math.abs(content.second - expectedSecond) <= CONTENT_SECOND_TOLERANCE
      }
    }
  }

  report.samples.push(entry)
  return entry
}

async function waitFor(description, timeoutMs, pollMs, fn) {
  const startedAt = performance.now()
  let last

  while (performance.now() - startedAt <= timeoutMs) {
    last = fn()
    if (last.ok) {
      report.checks.push({
        description,
        ok: true,
        elapsedMs: Math.round(performance.now() - startedAt),
        detail: last.detail ?? null
      })
      return last.detail
    }
    await sleep(pollMs)
  }

  fail(`Timed out waiting for ${description}`, {
    timeoutMs,
    last: last?.detail ?? null
  })
}

async function withNativeCallTimeout(description, promise, detail = {}) {
  let timer
  const task = Promise.resolve(promise)
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => {
      const error = new Error(`${description} timed out after ${NATIVE_CALL_TIMEOUT_MS}ms`)
      error.details = {
        timeoutMs: NATIVE_CALL_TIMEOUT_MS,
        ...detail
      }
      reject(error)
    }, NATIVE_CALL_TIMEOUT_MS)
  })

  try {
    return await Promise.race([task, timeout])
  } finally {
    clearTimeout(timer)
    task.catch(() => {})
  }
}

function summarizeWindow(samples) {
  if (samples.length < 2) {
    return {
      count: samples.length,
      wallDeltaMs: 0,
      progressDeltaMs: 0,
      rate: 0,
      maxBackwardMs: 0,
      maxStepExcessMs: 0
    }
  }

  let maxBackwardMs = 0
  let maxStepExcessMs = 0
  for (let i = 1; i < samples.length; i++) {
    const progressDelta = samples[i].progressMs - samples[i - 1].progressMs
    const wallDelta = samples[i].atMs - samples[i - 1].atMs
    if (progressDelta < 0) {
      maxBackwardMs = Math.max(maxBackwardMs, -progressDelta)
    }
    maxStepExcessMs = Math.max(maxStepExcessMs, progressDelta - wallDelta)
  }

  const first = samples[0]
  const last = samples[samples.length - 1]
  const wallDeltaMs = last.atMs - first.atMs
  const progressDeltaMs = last.progressMs - first.progressMs
  const rate = wallDeltaMs > 0 ? progressDeltaMs / wallDeltaMs : 0

  return {
    count: samples.length,
    wallDeltaMs,
    progressDeltaMs,
    rate,
    maxBackwardMs,
    maxStepExcessMs
  }
}

async function collectSteadyWindow(player, label) {
  const startedAt = performance.now()
  const windowSamples = []

  while (performance.now() - startedAt <= STEADY_WINDOW_MS) {
    windowSamples.push(sample(player, `${label}:steady`))
    await sleep(SAMPLE_MS)
  }

  const summary = summarizeWindow(windowSamples)
  const contentSamples = windowSamples.filter((entry) => entry.content?.ok)
  const contentMismatches = contentSamples.filter((entry) => !entry.content.matchesProgress)
  const contentMismatchRatio =
    contentSamples.length > 0 ? contentMismatches.length / contentSamples.length : 0
  const ok =
    summary.count >= 4 &&
    summary.rate >= STEADY_RATE_MIN &&
    summary.rate <= STEADY_RATE_MAX &&
    summary.maxBackwardMs <= MAX_STEADY_BACKWARD_MS &&
    summary.maxStepExcessMs <= MAX_STEADY_STEP_EXCESS_MS &&
    (!captureProbe || (contentSamples.length >= 4 && contentMismatchRatio <= CONTENT_MAX_MISMATCH_RATIO))

  report.checks.push({
    description: `${label} steady progress window`,
    ok,
    detail: {
      ...summary,
      contentSamples: contentSamples.length,
      contentMismatches: contentMismatches.length,
      contentMismatchRatio,
      contentMismatchExamples: contentMismatches.slice(0, 5)
    }
  })

  if (!ok) {
    fail(`${label} steady progress/content check failed`, {
      ...summary,
      contentSamples: contentSamples.length,
      contentMismatches: contentMismatches.length,
      contentMismatchRatio,
      contentMismatchExamples: contentMismatches.slice(0, 5)
    })
  }

  return windowSamples
}

function isNear(progressMs, targetMs) {
  return Math.abs(progressMs - targetMs) <= SEEK_ANCHOR_TOLERANCE_MS
}

async function waitForSeekAnchor(player, label, targetMs) {
  return waitFor(
    `${label} progress anchor near ${targetMs}ms`,
    SEEK_ANCHOR_TIMEOUT_MS,
    SAMPLE_MS,
    () => {
      const current = sample(player, `${label}:anchor`)
      return {
        ok: isNear(current.progressMs, targetMs),
        detail: current
      }
    }
  )
}

async function waitForUnbuffered(player, label) {
  return waitFor(`${label} buffering=false`, BUFFER_TIMEOUT_MS, SAMPLE_MS, () => {
    const current = sample(player, `${label}:buffer`)
    return {
      ok: current.isBuffering === false,
      detail: current
    }
  })
}

async function calibrateCaptureLag(player, label) {
  if (!captureProbe) return

  const lagCandidates = []
  await waitFor(`${label} monitor capture calibration`, CAPTURE_CALIBRATION_TIMEOUT_MS, SAMPLE_MS, () => {
    const current = sample(player, `${label}:capture-calibration`)
    const content = current.content
    if (!content?.ok) {
      return { ok: false, detail: current }
    }

    const lagMs = current.progressMs - content.second * 1000
    if (lagMs < -CONTENT_SECOND_TOLERANCE * 1000 || lagMs > CAPTURE_MAX_LAG_MS) {
      return {
        ok: false,
        detail: {
          ...current,
          candidateLagMs: lagMs,
          reason: 'capture lag outside configured bounds'
        }
      }
    }

    lagCandidates.push(lagMs)
    if (lagCandidates.length < 4) {
      return {
        ok: false,
        detail: {
          ...current,
          candidateLagMs: lagMs,
          candidates: lagCandidates
        }
      }
    }

    captureLagMs = Math.max(0, Math.round(median(lagCandidates.slice(-4))))
    return {
      ok: true,
      detail: {
        captureLagMs,
        candidates: lagCandidates.slice(-8),
        sample: current
      }
    }
  })
}

async function waitForCaptureSettle(label) {
  if (!captureProbe) return

  const settleMs = Math.min(
    CAPTURE_MAX_LAG_MS + CAPTURE_SETTLE_EXTRA_MS,
    captureLagMs + CONTENT_WINDOW_MS + CAPTURE_SETTLE_EXTRA_MS
  )
  if (settleMs > 0) {
    report.checks.push({
      description: `${label} monitor capture settle`,
      ok: true,
      detail: { settleMs, captureLagMs }
    })
    await sleep(settleMs)
  }
}

async function runSeekCase(player, label, targetMs) {
  console.log(`[seek] ${label}: ${targetMs}ms`)
  report.checks.push({
    description: `${label} seek issued`,
    ok: true,
    detail: { targetMs, before: sample(player, `${label}:before`) }
  })

  player.seek(targetMs / 1000)
  await waitForSeekAnchor(player, label, targetMs)
  await waitForUnbuffered(player, label)
  await waitForCaptureSettle(label)
  await collectSteadyWindow(player, label)
}

async function runRapidSeekCase(player, label = 'rapid-overwrite') {
  const firstTargetMs = 38_000
  const finalTargetMs = 9_000
  console.log(`[seek] ${label}: ${firstTargetMs}ms -> ${finalTargetMs}ms`)

  player.seek(firstTargetMs / 1000)
  await sleep(10)
  player.seek(finalTargetMs / 1000)

  await waitForSeekAnchor(player, label, finalTargetMs)
  await waitForUnbuffered(player, label)
  await waitForCaptureSettle(label)
  const windowSamples = await collectSteadyWindow(player, label)

  const wrongTargetSamples = windowSamples.filter((entry) => isNear(entry.progressMs, firstTargetMs))
  const ok = wrongTargetSamples.length === 0
  report.checks.push({
    description: `${label} never returns to first target after final target settles`,
    ok,
    detail: {
      firstTargetMs,
      finalTargetMs,
      wrongTargetSamples
    }
  })
  if (!ok) {
    fail('Rapid seek was overwritten by an older seek completion', {
      firstTargetMs,
      finalTargetMs,
      wrongTargetSamples
    })
  }
}

async function runStartAtPlaybackCase(player, audioCase, targetMs = START_AT_RESTORE_MS) {
  const label = `${audioCase.label}:startup-restore`
  console.log(`[start-at] ${label}: ${targetMs}ms`)

  try {
    await withNativeCallTimeout(
      `${label} playFile(start_at)`,
      player.playFile(audioCase.path, targetMs / 1000, false),
      { audioCase, targetMs }
    )
  } catch (error) {
    fail(`Native start_at playback probe could not start for ${audioCase.label}.`, {
      error: error.message,
      audioCase,
      targetMs
    })
  }

  await waitForSeekAnchor(player, label, targetMs)
  await waitForUnbuffered(player, label)
  await waitForCaptureSettle(label)
  await collectSteadyWindow(player, label)
}

async function runCachedUrlStartAtCase(player, audioCase, targetMs = START_AT_RESTORE_MS) {
  if (!runCachedUrlProbe) return

  const label = `${audioCase.label}:cached-url-startup-restore`
  const safeLabel = label.replace(/[^a-z0-9.-]+/gi, '-')
  const cachePath = path.join(path.dirname(audioCase.path), `${safeLabel}.cache`)
  const metadataPath = path.join(path.dirname(audioCase.path), `${safeLabel}.json`)
  const server = await startRangeServer(audioCase.path)
  console.log(
    `[cached-url] ${label}: ${targetMs}ms, maxWriteAhead=${CACHED_URL_MAX_WRITE_AHEAD_BYTES}`
  )
  fs.writeFileSync(
    metadataPath,
    JSON.stringify(
      {
        song_id: -1,
        quality: 'regression',
        source_url: server.url,
        content_length: null,
        downloaded_ranges: [],
        is_complete: false,
        updated_at: Math.floor(Date.now() / 1000)
      },
      null,
      2
    )
  )

  try {
    try {
      await withNativeCallTimeout(
        `${label} playUrlCached(start_at)`,
        player.playUrlCached(
          server.url,
          cachePath,
          metadataPath,
          DURATION_SECS * 1000,
          10,
          CACHED_URL_MAX_WRITE_AHEAD_BYTES,
          targetMs / 1000,
          false
        ),
        {
          audioCase,
          targetMs,
          cachePath,
          metadataPath,
          url: server.url
        }
      )
    } catch (error) {
      fail(`Native cached URL start_at probe could not start for ${audioCase.label}.`, {
        error: error.message,
        audioCase,
        targetMs,
        cachePath,
        metadataPath,
        url: server.url
      })
    }

    await waitForSeekAnchor(player, label, targetMs)
    await waitForUnbuffered(player, label)
    await waitForCaptureSettle(label)
    await collectSteadyWindow(player, label)
  } finally {
    await server.close()
    fs.rmSync(cachePath, { force: true })
    fs.rmSync(metadataPath, { force: true })
  }
}

async function runAudioProbeCase(player, audioCase) {
  console.log(`[audio] probing ${audioCase.label}: ${audioCase.path}`)
  captureLagMs = 0
  if (captureProbe) captureProbe.reset()

  try {
    await withNativeCallTimeout(
      `${audioCase.label} initial playFile`,
      player.playFile(audioCase.path, 0, false),
      { audioCase }
    )
  } catch (error) {
    fail(`Native audio probe could not start playback for ${audioCase.label}.`, {
      error: error.message,
      audioCase
    })
  }

  await waitFor(`${audioCase.label} initial playback progress`, 3_000, SAMPLE_MS, () => {
    const current = sample(player, `${audioCase.label}:startup`)
    return {
      ok: current.progressMs >= 250 && current.isBuffering === false,
      detail: current
    }
  })

  await calibrateCaptureLag(player, audioCase.label)
  await sleep(900)
  await runStartAtPlaybackCase(player, audioCase)
  await runCachedUrlStartAtCase(player, audioCase)
  await runSeekCase(player, `${audioCase.label}:forward-seek-after-buffer-fill`, 24_000)
  await runSeekCase(player, `${audioCase.label}:backward-seek`, 6_000)
  await runRapidSeekCase(player, `${audioCase.label}:rapid-overwrite`)
  await runSeekCase(player, `${audioCase.label}:late-forward-seek`, 44_000)
}

async function runAudioProbe() {
  const nativePath = path.join(NATIVE_DIR, 'index.node')
  if (!fs.existsSync(nativePath)) {
    fail('native/index.node is missing; run npm run build:native:dev first')
  }

  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ncm-native-seek-'))
  const wavPath = path.join(tempDir, 'seek-regression.wav')
  report.tempDir = tempDir
  report.wavPath = wavPath
  writeTestWav(wavPath)

  const audioCases = buildAudioCases(wavPath, tempDir)
  report.audioCases = audioCases
  console.log(`[audio] generated ${wavPath}`)

  if (captureMonitor) {
    const monitorSource = findDefaultMonitorSource()
    captureProbe = new MonitorCapture(monitorSource)
    captureProbe.start()
    await sleep(350)
    console.log(`[capture] recording monitor source ${monitorSource}`)
  }

  const { PlayerService } = require(nativePath)
  let player

  try {
    player = new PlayerService()
  } catch (error) {
    fail('Unable to create native PlayerService. A default output device is required for audio probe.', {
      error: error.message
    })
  }

  try {
    for (const audioCase of audioCases) {
      await runAudioProbeCase(player, audioCase)
    }
  } finally {
    if (player) {
      try {
        player.stop()
      } catch {
        // ignore cleanup failures
      }
    }
    if (captureProbe) {
      captureProbe.stop()
      captureProbe = null
    }
    if (!keepTemp) {
      fs.rmSync(tempDir, { recursive: true, force: true })
    }
  }
}

async function main() {
  if (args.has('--help') || args.has('-h')) {
    usage()
    return
  }

  if (runCargo) {
    runCommand('cargo', ['test'], { cwd: NATIVE_DIR })
  }

  if (runAudio && buildNative) {
    runCommand('npm', ['run', 'build:native:dev'])
  }

  if (runAudio) {
    await runAudioProbe()
  }

  fs.writeFileSync(reportPath, JSON.stringify(report, null, 2))
  console.log(`Report: ${reportPath}`)
  console.log('native seek regression: PASS')
}

main().catch((error) => {
  try {
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2))
    console.error(`Report: ${reportPath}`)
  } catch {
    // ignore report write failures
  }
  console.error(error.message)
  if (error.details) {
    console.error(JSON.stringify(error.details, null, 2))
  }
  process.exit(1)
})
