// Empty-home comparison fixture. Run through profile-memory-fixture.py.
const { app, BrowserWindow, ipcMain } = require('electron')
const path = require('node:path')
if (!process.env.NCM_MEMORY_USER_DATA) throw new Error('Use profile-memory-fixture.py --electron')
app.setPath('userData', process.env.NCM_MEMORY_USER_DATA)
app.commandLine.appendSwitch('ozone-platform', 'wayland')
process.on('SIGTERM', () => app.quit())
const root = path.resolve(__dirname, '../../electron')
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

app
  .whenReady()
  .then(async () => {
    const bodies = {
      recommendResource: { recommend: [] },
      recommendSongs: { data: { dailySongs: [] } },
      playlistCatlist: { sub: [] },
      banner: { banners: [] },
      userPlaylist: { playlist: [] },
      likelist: { ids: [] }
    }
    for (const [name, body] of Object.entries(bodies)) {
      ipcMain.handle(`music:${name}`, () => ({ status: 200, body: { code: 200, ...body } }))
    }
    const handlers = {
      'player:getPlaybackStatus': () => ({ isPlaying: false, isBuffering: false, progressMs: 0 }),
      'player:getOutputDevices': () => [],
      'player:setVolume': () => undefined,
      'player:stop': () => undefined,
      'player:waitFinished': () => new Promise(() => {}),
      'cache:getStats': () => ({
        totalBytes: 0,
        maxSizeBytes: 2147483648,
        songBytes: 0,
        songEntries: 0,
        entityBytes: 0,
        entityEntries: 0,
        coverBytes: 0,
        coverEntries: 0,
        lyricBytes: 0,
        lyricEntries: 0
      }),
      'cache:getSongMaxCacheAheadBytes': () => 33554432,
      'cache:resolveCachedMediaUrl': (_, url) => url,
      'library:checkFiles': () => [],
      'ui:getAppInfo': () => ({ name: 'NCM Desktop for Linux', version: 'memory-fixture' }),
      'ui:setWindowBackground': () => undefined
    }
    for (const [channel, handler] of Object.entries(handlers)) ipcMain.handle(channel, handler)
    const win = new BrowserWindow({
      width: 1435,
      height: 1000,
      minWidth: 1340,
      minHeight: 960,
      useContentSize: true,
      frame: false,
      show: true,
      webPreferences: { preload: path.join(root, 'out/preload/index.js'), sandbox: false }
    })
    win.webContents.on('console-message', (event) => console.log('renderer:', event.message))
    win.webContents.session.webRequest.onBeforeRequest(
      { urls: ['http://*/*', 'https://*/*'] },
      (_, callback) => callback({ cancel: true })
    )
    await win.loadFile(path.join(root, 'out/renderer/index.html'))
    await wait(12000)
    console.log(
      'MEMORY_READY',
      JSON.stringify({
        pid: process.pid,
        metrics: app.getAppMetrics(),
        gpu: app.getGPUFeatureStatus(),
        bounds: win.getContentBounds()
      })
    )
    // The parent sampler normally terminates this isolated process tree first.
    await wait(30000)
    app.quit()
  })
  .catch((error) => {
    console.error(error)
    app.exit(1)
  })
