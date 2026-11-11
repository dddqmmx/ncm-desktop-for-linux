#!/usr/bin/env node
/**
 * 获取本软件当前登录的 cookie（本地提取，零依赖）。
 *
 * 原理：本软件登录态存在 renderer localStorage key `app_cookie`，
 * Electron 会将其持久化到 userData 下 `Local Storage/leveldb/*.log|*.ldb`，
 * 内容里包含 `MUSIC_U=...`。本脚本只做本地二进制扫描，不联网。
 *
 * 安全设计：
 * - 默认不打印 cookie 全文，只打印长度/来源/掩码预览
 * - 全文只写到 --out 文件（默认 os.tmpdir()/ncm-cookie.txt），权限 0600
 * - 只有显式传 --print 才会输出全文（不建议截图/粘贴到聊天）
 *
 * 用法：
 *   node scripts/get-login-cookie.cjs [--out /tmp/ncm-cookie.txt] [--print] [--dir <userData|leveldb目录>]
 *   ELECTRON_LOCAL_STORAGE_DIR=/path/to/leveldb node scripts/get-login-cookie.cjs
 *   ELECTRON_USER_DATA_DIR=/path/to/userData node scripts/get-login-cookie.cjs
 */

const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')

function parseArgs(argv) {
  const opts = { out: path.join(os.tmpdir(), 'ncm-cookie.txt'), print: false, dir: '', help: false }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a === '--out' && argv[i + 1]) opts.out = argv[++i]
    else if (a === '--dir' && argv[i + 1]) opts.dir = argv[++i]
    else if (a === '--print') opts.print = true
    else if (a === '-h' || a === '--help') opts.help = true
  }
  if (process.env.NCM_COOKIE_OUT) opts.out = process.env.NCM_COOKIE_OUT
  return opts
}

function candidateLevelDbDirs(extraDir) {
  const home = os.homedir()
  const xdg = process.env.XDG_CONFIG_HOME || path.join(home, '.config')
  const dirs = []
  if (process.env.ELECTRON_LOCAL_STORAGE_DIR) dirs.push(process.env.ELECTRON_LOCAL_STORAGE_DIR)
  if (extraDir) {
    // 允许直接传 userData 或 leveldb 目录
    if (extraDir.endsWith('leveldb')) dirs.push(extraDir)
    else {
      dirs.push(path.join(extraDir, 'Local Storage', 'leveldb'))
      dirs.push(extraDir)
    }
  }
  if (process.env.ELECTRON_USER_DATA_DIR) {
    dirs.push(path.join(process.env.ELECTRON_USER_DATA_DIR, 'Local Storage', 'leveldb'))
  }
  // 本仓库已知位置（见 scripts/reproduce-song-*.cjs）+ productName 变体
  const names = ['ncm-desktop-for-linux', 'music', 'Music', 'Electron', 'com.electron.app']
  for (const base of new Set([xdg, path.join(home, '.config')])) {
    for (const n of names) dirs.push(path.join(base, n, 'Local Storage', 'leveldb'))
  }
  // snap 常见位置
  dirs.push(
    path.join(
      home,
      'snap',
      'ncm-desktop-for-linux',
      'common',
      '.config',
      'ncm-desktop-for-linux',
      'Local Storage',
      'leveldb'
    )
  )
  return [...new Set(dirs.filter(Boolean))]
}

function candidateCookieDbs(extraDir) {
  const home = os.homedir()
  const xdg = process.env.XDG_CONFIG_HOME || path.join(home, '.config')
  const userDatas = []
  if (extraDir && !extraDir.endsWith('leveldb')) userDatas.push(extraDir)
  if (process.env.ELECTRON_USER_DATA_DIR) userDatas.push(process.env.ELECTRON_USER_DATA_DIR)
  for (const base of [xdg]) {
    for (const n of ['ncm-desktop-for-linux', 'music', 'Electron'])
      userDatas.push(path.join(base, n))
  }
  const files = []
  for (const ud of new Set(userDatas)) {
    files.push(path.join(ud, 'Cookies'), path.join(ud, 'Network', 'Cookies'))
  }
  return files
}

function isCookieByte(b) {
  return b >= 0x20 && b <= 0x7e
}

function extractCookieAround(buffer, markerIndex) {
  let start = markerIndex
  while (start > 0 && isCookieByte(buffer[start - 1])) start--
  let end = markerIndex
  while (end < buffer.length && isCookieByte(buffer[end])) end++
  const value = buffer.toString('utf8', start, end).trim()
  const at = value.indexOf('MUSIC_U=')
  if (at < 0) return ''
  const cookie = value.slice(at)
  // 截掉 LevelDB 后缀的 origin/meta（沿用仓库已有启发式，避免把 file:///_https:// 等尾巴带进来）
  const cut = cookie.search(/(?:\\x00|_https?:|file:\/\/|app_cookie|currentSong)/)
  return (cut > 0 ? cookie.slice(0, cut) : cookie).trim()
}

function extractAllFromFile(filePath) {
  let buffer
  try {
    buffer = fs.readFileSync(filePath)
  } catch {
    return []
  }
  const marker = Buffer.from('MUSIC_U=', 'utf8')
  const found = []
  let offset = 0
  while (offset < buffer.length) {
    const idx = buffer.indexOf(marker, offset)
    if (idx < 0) break
    const c = extractCookieAround(buffer, idx)
    if (c.includes('MUSIC_U=') && c.length > 20) found.push(c)
    offset = idx + marker.length
  }
  return found
}

function maskedPreview(cookie) {
  // 只显示掩码，不泄露真实值：MUSIC_U=****(共N字符, M段)
  const segs = cookie.split(';').filter((s) => s.trim()).length
  return `MUSIC_U=****…**** (共 ${cookie.length} 字符, ${segs} 段)`
}

function main() {
  const opts = parseArgs(process.argv.slice(2))
  if (opts.help) {
    console.log(
      '用法: node scripts/get-login-cookie.cjs [--out /tmp/ncm-cookie.txt] [--print] [--dir <userData|leveldb>]'
    )
    console.log('默认只写文件不打印全文；需要查看全文必须显式加 --print。')
    return
  }

  const dirs = candidateLevelDbDirs(opts.dir)
  const checked = []
  const candidates = [] // {cookie, source}
  for (const dir of dirs) {
    if (!fs.existsSync(dir)) {
      checked.push(`${dir} (不存在)`)
      continue
    }
    checked.push(dir)
    let files = []
    try {
      files = fs
        .readdirSync(dir)
        .filter((n) => /\.(?:log|ldb|sst)$/i.test(n))
        .map((n) => path.join(dir, n))
        .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs)
    } catch (e) {
      console.error(`[跳过] 无法列目录 ${dir}: ${e.message}`)
      continue
    }
    for (const f of files) {
      for (const c of extractAllFromFile(f)) candidates.push({ cookie: c, source: f })
    }
  }

  // 兜底：Electron session Cookies sqlite 也做同样的二进制扫描（无需 sqlite3 依赖）
  if (candidates.length === 0) {
    for (const f of candidateCookieDbs(opts.dir)) {
      if (!fs.existsSync(f)) continue
      checked.push(f)
      for (const c of extractAllFromFile(f)) candidates.push({ cookie: c, source: f })
    }
  }

  // 去重 + 选最长的（通常是最完整的 app_cookie）
  const seen = new Set()
  const uniq = candidates.filter((c) => (seen.has(c.cookie) ? false : (seen.add(c.cookie), true)))
  uniq.sort((a, b) => b.cookie.length - a.cookie.length)
  const best = uniq[0]

  console.log('已检查位置:')
  for (const d of checked) console.log(` - ${d}`)
  console.log(`候选 cookie 数量: ${uniq.length}`)
  if (uniq.length > 1) {
    console.log('各候选长度:', uniq.map((c) => c.cookie.length).join(', '))
  }

  if (!best) {
    console.error(
      '\n未找到 MUSIC_U，请确认：1) 软件已登录 2) 登录后完全退出过一次(落盘) 3) 用 --dir 指定 userData 目录。'
    )
    console.error('可先关闭软件再重试，或设置 ELECTRON_LOCAL_STORAGE_DIR 再跑。')
    process.exitCode = 1
    return
  }

  // 写文件，0600
  fs.mkdirSync(path.dirname(opts.out), { recursive: true })
  fs.writeFileSync(opts.out, best.cookie, { mode: 0o600 })
  try {
    fs.chmodSync(opts.out, 0o600)
  } catch {
    // Windows 下忽略 chmod
  }

  console.log(`\n来源文件: ${best.source}`)
  console.log(`预览: ${maskedPreview(best.cookie)}`)
  console.log(`已写入: ${opts.out} (权限 0600)`)
  console.log('使用示例: COOKIE_FILE=/tmp/ncm-cookie.txt node scripts/reproduce-song-*.cjs')
  console.log('注意: 不要把该文件内容粘贴到聊天/截图/日志里。')

  if (opts.print) {
    console.log('\n---- COOKIE BEGIN (你显式要求 --print) ----')
    console.log(best.cookie)
    console.log('---- COOKIE END ----')
  }
}

main()
