/* eslint-disable */
/**
 * 在 Electron 主窗口的 DevTools Console 里粘贴运行，验证 WebAPI 引擎的缓存解析链路。
 *
 * 步骤：
 * 1. 先播放一首歌（走 WebAPI 引擎），让它产生本地文件缓存；
 * 2. 打开 DevTools → Console，粘贴本脚本；
 * 3. 把下方的 songId / url 改成实际想测的歌曲；
 * 4. 回车运行，观察日志。
 */

async function testWebAudioCache() {
  // 改成你想测试的歌曲
  const songId = 0 // 例如：1901371647
  const quality = 'exhigh'
  const url = '' // 例如：https://m701.music.126.net/.../xxx.mp3

  if (!songId || !url) {
    console.error('请先把 songId 和 url 换成实际值再运行')
    return
  }

  console.log('[1/4] 测试网络 URL 能否被 <audio> 直接解析...')
  const netAudio = new Audio(url)
  const netOk = await new Promise((resolve) => {
    netAudio.addEventListener('loadedmetadata', () => {
      console.log('  ✅ 网络 URL 可解析，duration:', netAudio.duration)
      resolve(true)
    })
    netAudio.addEventListener('error', () => {
      console.error('  ❌ 网络 URL 解析失败:', netAudio.error)
      resolve(false)
    })
    netAudio.load()
  })
  if (!netOk) return

  console.log('[2/4] 调用 prepare_cached_song_source 准备缓存源...')
  const source = await window.api.prepare_cached_song_source({
    songId,
    quality,
    url,
    expectedBytes: undefined
  })
  console.log('  source:', source)

  if (source.type === 'file' && source.value) {
    const cacheUrl = `ncm-cache://asset?path=${encodeURIComponent(source.value)}`
    console.log('[3/4] 命中本地缓存，测试 ncm-cache URL:', cacheUrl)

    const cacheAudio = new Audio(cacheUrl)
    const cacheOk = await new Promise((resolve) => {
      cacheAudio.addEventListener('loadedmetadata', () => {
        console.log('  ✅ ncm-cache URL 可解析，duration:', cacheAudio.duration)
        resolve(true)
      })
      cacheAudio.addEventListener('canplay', () => {
        console.log('  ℹ️  ncm-cache URL canplay，准备就绪')
      })
      cacheAudio.addEventListener('error', () => {
        console.error('  ❌ ncm-cache URL 解析失败:', cacheAudio.error)
        resolve(false)
      })
      // 给个超时兜底
      setTimeout(() => {
        console.warn('  ⚠️  ncm-cache URL 10秒无响应')
        resolve(false)
      }, 10000)
      cacheAudio.load()
    })

    if (cacheOk) {
      console.log('[4/4] 尝试实际播放 ncm-cache URL...')
      await cacheAudio.play().catch((err) => {
        console.error('  ❌ 播放失败:', err)
      })
    }
  } else if (source.type === 'url' && source.value) {
    console.log('[3/4] 未命中本地文件缓存，启动后台完整缓存下载:', source.value)
    console.log('  cachePath:', source.cachePath)
    console.log('  metadataPath:', source.metadataPath)

    await window.api
      .cache_song_source({
        songId,
        quality,
        url,
        expectedBytes: undefined,
        durationMs: undefined
      })
      .then((cached) => {
        console.log('[4/4] 后台缓存完成:', cached)
        if (cached.type === 'file' && cached.value) {
          const cacheUrl = `ncm-cache://asset?path=${encodeURIComponent(cached.value)}`
          console.log('  现在可以直接播放 ncm-cache URL:', cacheUrl)
        }
      })
      .catch((err) => console.error('  ❌ 后台缓存失败:', err))
  } else {
    console.error('  ❌ prepare_cached_song_source 返回无效 source')
  }
}

testWebAudioCache().catch(console.error)
