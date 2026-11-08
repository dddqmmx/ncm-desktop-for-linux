const { app, BrowserWindow, ipcMain } = require('electron')
const fs = require('node:fs')
const path = require('node:path')

app.disableHardwareAcceleration()
app.commandLine.appendSwitch('ozone-platform', 'headless')
app.commandLine.appendSwitch('no-sandbox')

const ROOT = path.resolve(__dirname, '../..')
const PRELOAD_PATH = path.join(ROOT, 'out/preload/index.js')
const RENDERER_HTML = path.join(ROOT, 'out/renderer/index.html')
const OUTPUT_JSON = path.join(__dirname, '../tests/electron-ground-truth.json')
const PLAYLIST_ONLY = process.argv.includes('--playlist-management')
const SETTINGS_ONLY = process.argv.includes('--settings-window')
const HOVER_ONLY = process.argv.includes('--sidebar-hover')
const LOGIN_ONLY = process.argv.includes('--login-modal')
const FULLSCREEN_ONLY = process.argv.includes('--fullscreen-player')
const ARTIST_ONLY = process.argv.includes('--artist-view')
let artistFixture
if (ARTIST_ONLY) app.setPath('userData', fs.mkdtempSync('/tmp/ncm-artist-electron-'))
const HOME_ONLY = process.argv.includes('--home-view')
if (HOME_ONLY) app.setPath('userData', fs.mkdtempSync('/tmp/ncm-home-electron-'))
if (FULLSCREEN_ONLY) app.setPath('userData', fs.mkdtempSync('/tmp/ncm-fullscreen-electron-'))

const DATA_SVG =
  'data:image/svg+xml;utf8,' +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 400"><rect width="400" height="400" fill="#6366f1"/></svg>'
  )

const MOCK_TRACKS = Array.from({ length: 8 }, (_, idx) => ({
  id: idx + 1,
  name: `Midnight City Lights ${idx + 1}`,
  dt: 243000,
  ar: [{ id: 100, name: 'M83' }],
  al: { id: 200, name: "Hurry Up, We're Dreaming", picUrl: DATA_SVG },
  artists: [{ id: 100, name: 'M83' }],
  album: { id: 200, name: "Hurry Up, We're Dreaming", picUrl: DATA_SVG },
  duration: 243000
}))

function registerMockIpc() {
  const handlers = {
    'music:login': () => ({ body: { code: 502, message: '账号或密码错误' } }),
    'music:captcha_sent': () => ({ body: { code: 200 } }),
    'music:configureXeapi': () => ({ body: { code: 200 } }),
    'music:userAccount': () => ({
      body: {
        code: 200,
        account: { id: 10001 },
        profile: { userId: 10001, nickname: '云村漫游者', avatarUrl: DATA_SVG }
      }
    }),
    'music:userPlaylist': () => ({
      body: {
        code: 200,
        playlist: [
          {
            id: 101,
            specialType: 0,
            name: '深夜编码 · 赛博合成器浪潮',
            trackCount: 28,
            playCount: 364000,
            SubscribedCount: 128000,
            coverImgUrl: DATA_SVG,
            creator: { userId: 10001, nickname: '云村漫游者', avatarUrl: DATA_SVG }
          },
          {
            id: 102,
            name: '冬日暖阳 · 独立民谣与原声',
            trackCount: 45,
            playCount: 121000,
            coverImgUrl: DATA_SVG,
            creator: { userId: 10001, nickname: '云村漫游者', avatarUrl: DATA_SVG }
          },
          {
            id: 103,
            name: 'Tokyo City Pop 1984',
            trackCount: 32,
            playCount: 892000,
            coverImgUrl: DATA_SVG,
            creator: { userId: 10001, nickname: '云村漫游者', avatarUrl: DATA_SVG }
          },
          {
            id: 104,
            name: 'Post-Rock 器乐后摇精选',
            trackCount: 19,
            playCount: 54000,
            coverImgUrl: DATA_SVG,
            creator: { userId: 10001, nickname: '云村漫游者', avatarUrl: DATA_SVG }
          },
          {
            id: 105,
            name: '雨夜爵士咖啡馆 Lo-Fi',
            trackCount: 64,
            playCount: 230000,
            coverImgUrl: DATA_SVG,
            creator: { userId: 10001, nickname: '云村漫游者', avatarUrl: DATA_SVG }
          }
        ]
      }
    }),
    'music:likelist': () => ({ body: { code: 200, ids: [1, 2, 4, 6, 8] } }),
    'music:songDetail': () => ({ body: { code: 200, songs: MOCK_TRACKS } }),
    'music:recommendResource': () => ({
      body: {
        code: 200,
        recommend: Array.from({ length: 6 }, (_, i) => ({
          id: 101 + i,
          name: `推荐歌单 ${i + 1}`,
          copywriter: '根据你常听的电子与器乐推荐',
          picUrl: DATA_SVG,
          playcount: 100000,
          creator: { userId: 10001, nickname: '云村漫游者', avatarUrl: DATA_SVG }
        }))
      }
    }),
    'music:recommendSongs': () => ({
      body: {
        code: 200,
        data: {
          dailySongs: MOCK_TRACKS
        }
      }
    }),
    'music:playlistCatlist': () => ({
      body: {
        code: 200,
        sub: [
          { name: '流行' },
          { name: '摇滚' },
          { name: '电子' },
          { name: '民谣' },
          { name: '轻音乐' },
          { name: '影视原声' },
          { name: 'ACG' },
          { name: '爵士' },
          { name: '古典' },
          { name: '说唱' },
          { name: '怀旧' },
          { name: '治愈' }
        ]
      }
    }),
    'music:search': () => ({
      body: {
        result: {
          songCount: 8,
          songs: MOCK_TRACKS
        }
      }
    }),
    'music:playlistDetail': () => ({
      body: {
        code: 200,
        playlist: {
          id: 101,
          specialType: 0,
          name: '深夜编码 · 赛博合成器浪潮',
          coverImgUrl: DATA_SVG,
          description: '精选 Synthwave、Dream Pop 与电子器乐，伴你在霓虹夜色与终端微光中保持专注。',
          createTime: 1779062400000,
          trackCount: 28,
          playCount: 364000,
          subscribedCount: 128000,
          creator: { userId: 10001, nickname: '云村漫游者', avatarUrl: DATA_SVG },
          tracks: MOCK_TRACKS,
          trackIds: [{ id: 1 }]
        }
      }
    }),
    'music:album': () => ({
      body: {
        code: 200,
        album: {
          id: 200,
          name: "Hurry Up, We're Dreaming",
          picUrl: DATA_SVG,
          description: '法国电子乐团 M83 的第六张录音室双碟专辑。',
          publishTime: 1318896000000,
          size: 22,
          company: 'Naive Records',
          artists: [{ id: 100, name: 'M83' }]
        },
        songs: MOCK_TRACKS
      }
    }),
    'music:artistDetail': () => artistFixture ? ({ body: artistFixture.detail }) : ({
      body: {
        code: 200,
        data: {
          artist: {
            id: 100,
            name: 'M83',
            avatar: DATA_SVG,
            cover: DATA_SVG,
            albumSize: 8,
            mvSize: 16,
            briefDesc: '法国电子与梦幻流行乐团，由 Anthony Gonzalez 于 1999 年在昂蒂布创立。'
          }
        }
      }
    }),
    'music:artistTopSong': () => artistFixture ? ({ body: artistFixture.top }) : ({
      body: { code: 200, songs: MOCK_TRACKS.slice(0, 5) }
    }),
    'music:artistAlbum': () => artistFixture ? ({ body: artistFixture.album }) : ({
      body: {
        code: 200,
        hotAlbums: [
          { id: 401, name: 'Fantasy', picUrl: DATA_SVG, publishTime: 1679011200000, type: 'Album' },
          { id: 402, name: 'DSVII', picUrl: DATA_SVG, publishTime: 1568937600000, type: 'Album' },
          { id: 411, name: 'Oceans Niagara', picUrl: DATA_SVG, publishTime: 1675036800000, type: 'Single' },
          { id: 412, name: 'Mirror', picUrl: DATA_SVG, publishTime: 1676036800000, type: 'EP' }
        ]
      }
    }),
    'music:artistMv': () => artistFixture ? ({ body: artistFixture.mv }) : ({
      body: {
        code: 200,
        mvs: [
          { id: 421, name: 'Midnight City', imgurl16v9: DATA_SVG, publishTime: '2011-10-18' },
          { id: 422, name: 'Oceans Niagara', imgurl16v9: DATA_SVG, publishTime: '2023-02-07' }
        ]
      }
    }),
    'music:lyric': () => ({
      body: {
        code: 200,
        lrc: {
          lyric:
            '[00:12.00]Waiting in a car\n[00:24.00]Waiting for a ride in the dark\n[00:48.00]The night city grows\n[01:32.00]Look at the horizon glow\n[01:48.00]Neon reflections on the wet asphalt\n' +
            (FULLSCREEN_ONLY ? '[00:00.00]\n[00:36.00]  \t\n[01:20.00]\n[02:00.00]\n' : '')
        },
        tlyric: {
          lyric:
            '[00:12.00]在车里静静等候\n[00:24.00]等待在夜色中启程\n[00:48.00]午夜的城市渐渐苏醒\n[01:32.00]看那地平线上泛起的微光\n[01:48.00]潮湿柏油路面上映出霓虹倒影\n'
        },
        romalrc: {
          lyric:
            '[00:12.00]wei-ting in a ka:\n[00:24.00]wei-ting fo: a raid in the da:k\n[00:48.00]the nait si-ti grouz\n[01:32.00]luk at the ho-rai-zen glou\n[01:48.00]ni-on ri-flek-shenz on the wet as-folt\n'
        }
      }
    }),
    'music:commentMusic': () => ({
      body: {
        code: 200,
        more: false,
        hotComments: [
          {
            commentId: 501,
            content: '在 Linux 桌面上用原生 ALSA BitPerfect 听这首合成器开场。',
            timeStr: '2026-10-12',
            likedCount: 24000,
            user: { nickname: '星河观测员', avatarUrl: DATA_SVG, vipType: 11 },
            beReplied: []
          }
        ],
        comments: [
          {
            commentId: 502,
            content: '每次写代码到凌晨两点放到这一首，精神立刻重新聚焦。',
            timeStr: '12分钟前',
            likedCount: 8912,
            user: { nickname: 'Synthwave_84', avatarUrl: DATA_SVG, vipType: 11 },
            beReplied: [{ content: '同感！', user: { nickname: '深海潜水艇' } }]
          }
        ]
      }
    }),
    'music:loginQrKey': () => ({ body: { data: { unikey: 'demo-key' } } }),
    'music:loginQrCreate': () => ({ body: { data: { qrimg: DATA_SVG } } }),
    'music:loginQrCheck': () => ({ body: { code: 801 } }),
    'cache:resolveCachedMediaUrl': (_, url) => url || DATA_SVG,
    'cache:getStats': () => ({
      totalBytes: 740712448,
      maxSizeBytes: 2147483648,
      songBytes: 674025472,
      songEntries: 38,
      entityBytes: 14889779,
      entityEntries: 64,
      coverBytes: 50960793,
      coverEntries: 126,
      lyricBytes: 839680,
      lyricEntries: 92
    }),
    'cache:getSongMaxCacheAheadBytes': () => 33554432,
    'player:getOutputDevices': () => [
      { id: 'hw:0,0', name: 'USB Hi-Fi DAC Audio', isDefault: false, isCurrent: true }
    ],
    'player:getPlaybackStatus': () => ({
      isPlaying: true,
      isBuffering: false,
      progressMs: 92000,
      bufferedPercent: 68
    }),
    'player:setVolume': () => undefined,
    'player:stop': () => undefined,
    'player:pause': () => undefined,
    'player:resume': () => undefined,
    'player:seek': () => undefined,
    'player:waitFinished': () => new Promise(() => {}),
    'ui:getAppInfo': () => ({ name: 'NCM Desktop for Linux', version: '1.0.0' }),
    'ui:setWindowBackground': () => undefined,
    'ui:openDialogWindow': () => {
      throw new Error('fallback to AppDialog')
    },
    'library:checkFiles': () => [
      '/home/example/Music/debussy_clair_de_lune.flac',
      '/home/example/Music/sakamoto_lawrence_master.flac'
    ]
  }

  for (const [channel, fn] of Object.entries(handlers)) {
    ipcMain.handle(channel, async (...args) => {
      const result = await fn(...args)
      return channel.startsWith('music:') ? { status: 200, ...result } : result
    })
  }
}

const wait = (ms) => new Promise((r) => setTimeout(r, ms))

app.whenReady().then(async () => {
  registerMockIpc()

  const win = new BrowserWindow({
    width: 1435,
    height: 1000,
    useContentSize: true,
    show: false,
    frame: false,
    webPreferences: {
      preload: PRELOAD_PATH,
      offscreen: true,
      sandbox: false
    }
  })
  if (PLAYLIST_ONLY) win.webContents.on('console-message', (event) => console.log(event.message))

  await win.loadFile(RENDERER_HTML)
  await wait(300)

  const seedScript = `
    (() => {
      const svg = ${JSON.stringify(DATA_SVG)};
      const currentSong = {
        id: 1,
        name: 'Midnight City Lights',
        artists: [{ id: 100, name: 'M83' }, { id: 101, name: '落日飞车' }],
        cover: svg,
        duration: 243000
      };
      const queue = [
        currentSong,
        { id: 2, name: 'Oceanic Horizon', artists: [{ id: 102, name: 'Tycho' }], cover: svg, duration: 282000 },
        { id: 3, name: 'Golden Hour Glow', artists: [{ id: 103, name: 'The Midnight' }], cover: svg, duration: 316000 }
      ];
      localStorage.setItem('app_cookie', 'MUSIC_U=demo;');
      localStorage.setItem('currentSong', JSON.stringify(currentSong));
      localStorage.setItem('currentSongId', '1');
      localStorage.setItem('currentTime', '92000');
      localStorage.setItem('playlist', JSON.stringify(queue));
      localStorage.setItem('playMode', 'loop');
      localStorage.setItem('app_settings', JSON.stringify({
        softwareVolume: true,
        volumePercent: 76,
        theme: 'light',
        libPaths: ['/home/example/Music/FLAC-Library', '/mnt/media/NeteaseCloudMusic']
      }));
      localStorage.setItem('localMusicLibrary', JSON.stringify([
        {
          id: -101,
          source: 'local',
          filePath: '/home/example/Music/debussy_clair_de_lune.flac',
          fileName: 'debussy_clair_de_lune.flac',
          name: 'Clair de Lune (24bit/96kHz)',
          artists: [{ id: 0, name: 'Claude Debussy' }],
          cover: '',
          duration: 308000
        }
      ]));
      return true;
    })();
  `
  await win.webContents.executeJavaScript(seedScript)
  await win.webContents.reload()
  await wait(550)

  const collectHelper = `
    window.__collectRectAndStyle = (selector) => {
      const el = document.querySelector(selector);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      const s = window.getComputedStyle(el);
      return {
        rect: {
          x: Math.round(r.x * 100) / 100,
          y: Math.round(r.y * 100) / 100,
          width: Math.round(r.width * 100) / 100,
          height: Math.round(r.height * 100) / 100
        },
        style: {
          borderRadius: s.borderRadius,
          fontSize: s.fontSize,
          fontWeight: s.fontWeight,
          paddingTop: s.paddingTop,
          paddingRight: s.paddingRight,
          paddingBottom: s.paddingBottom,
          paddingLeft: s.paddingLeft,
          gap: s.gap,
          columnGap: s.columnGap,
          rowGap: s.rowGap,
          opacity: s.opacity,
          transitionDuration: s.transitionDuration,
          transitionTimingFunction: s.transitionTimingFunction,
          transitionProperty: s.transitionProperty,
          animationName: s.animationName,
          animationDuration: s.animationDuration,
          animationTimingFunction: s.animationTimingFunction
        }
      };
    };

    // Sample a real Chromium Web Animation easing curve at exact normalized times [0, 0.25, 0.5, 0.75, 1.0]
    window.__sampleChromiumCurve = (durationMs, easingStr, fromVal, toVal) => {
      const probe = document.createElement('div');
      probe.style.position = 'fixed';
      probe.style.top = '0px';
      probe.style.left = '0px';
      document.body.appendChild(probe);
      const anim = probe.animate(
        [{ transform: 'translateX(' + fromVal + 'px)' }, { transform: 'translateX(' + toVal + 'px)' }],
        { duration: durationMs, easing: easingStr, fill: 'both' }
      );
      anim.pause();
      const fractions = [0, 0.25, 0.5, 0.75, 1.0];
      const samples = fractions.map((f) => {
        const t = Math.round(durationMs * f);
        anim.currentTime = t;
        const m = new DOMMatrixReadOnly(window.getComputedStyle(probe).transform);
        return {
          tMs: t,
          value: Math.round(m.m41 * 1000) / 1000
        };
      });
      anim.cancel();
      probe.remove();
      return {
        durationMs,
        easing: easingStr,
        from: fromVal,
        to: toVal,
        samples
      };
    };
    true;
  `
  await win.webContents.executeJavaScript(collectHelper)

  if (ARTIST_ONLY) {
    const report = { cases: [] }
    const folder = process.env.NCM_ELECTRON_SCREENSHOT_DIR
    if (folder) fs.mkdirSync(folder, { recursive: true })
    const fixtures = [
      { name: 'short', songs: 5, albums: 2, eps: 2, mvs: 2, summary: '法国电子与梦幻流行乐团，由 Anthony Gonzalez 于 1999 年在昂蒂布创立。' },
      { name: 'long', songs: 50, albums: 12, eps: 7, mvs: 7, summary: '这是一段用于检查歌手介绍自动换行的文字。音乐作品涵盖电子、流行与电影配乐，持续探索不同的声音和表达方式。'.repeat(5) },
      { name: 'empty', songs: 0, albums: 0, eps: 0, mvs: 0, summary: '' }
    ]
    let routeId = 100
    for (const spec of fixtures) {
      const album = (i, ep) => ({ id: (ep ? 500 : 400) + i, name: i === 1 ? '一张标题很长的专辑用于检查换行和下一行之间的间距' : (ep ? 'Single ' : 'Album ') + (i + 1), picUrl: DATA_SVG, publishTime: Date.UTC(2024 - i, 2, 1), type: ep ? 'EP' : '专辑' })
      artistFixture = {
        detail: { data: { artist: { id: routeId, name: 'M83', avatar: DATA_SVG, albumSize: spec.albums + spec.eps, mvSize: spec.mvs, briefDesc: spec.summary } } },
        top: { songs: Array.from({length: spec.songs}, (_, i) => ({...MOCK_TRACKS[i % MOCK_TRACKS.length], id: i + 1})) },
        album: { hotAlbums: [...Array.from({length: spec.albums}, (_, i) => album(i, false)), ...Array.from({length: spec.eps}, (_, i) => album(i, true))] },
        mv: { mvs: Array.from({length: spec.mvs}, (_, i) => ({ id: 600 + i, name: i === 1 ? '一段标题很长的音乐录像用于检查两行标题的展示效果' : 'Music Video ' + (i + 1), imgurl16v9: DATA_SVG, publishTime: '2023-02-07' })) }
      }
      for (const width of [1340, 1435, 1800]) {
        win.setContentSize(width, 1000)
        await win.webContents.executeJavaScript(`window.location.hash = '#/artist/${++routeId}'; true;`)
        await wait(500)
        await win.webContents.executeJavaScript(`document.querySelector('.artist-page .scroll-container').scrollTop = 0; true;`)
        await wait(80)
        const geometry = await win.webContents.executeJavaScript(`(() => {
          const rect = el => { const r = el.getBoundingClientRect(); return { x:r.x, y:r.y, width:r.width, height:r.height } }
          const one = selector => { const el = document.querySelector(selector); if (!el) throw Error('Missing ' + selector); return rect(el) }
          return {
            main: one('.artist-page .main-content'), header: one('.artist-header'), avatar: one('.artist-avatar-col'),
            name: one('.artist-name'), summary: one('.artist-summary'), pills: one('.pill-row'), actions: one('.cta-row'),
            sections: [...document.querySelectorAll('.artist-page section')].map(section => ({ title:section.querySelector('h2').textContent, ...rect(section), items:[...section.querySelectorAll('.latest-item, .song-row, .media-tile')].map(rect), covers:[...section.querySelectorAll('.latest-img-wrap-wrapper, .song-thumb-wrapper, .tile-img-wrap-wrapper')].map(rect) })),
            songsCard: document.querySelector('.songs-card') ? one('.songs-card') : null,
            scrollHeight: document.querySelector('.artist-page .scroll-container').scrollHeight
          }
        })()`)
        report.cases.push({ name:spec.name, width, fixture:artistFixture, geometry })
        if (folder) {
          fs.writeFileSync(path.join(folder, `artist-${spec.name}-${width}.png`), (await win.webContents.capturePage()).toPNG())
          if (spec.name === 'long') {
            await win.webContents.executeJavaScript(`document.querySelector('.media-grid').scrollIntoView(); true;`)
            await wait(100)
            fs.writeFileSync(path.join(folder, `artist-grid-${width}.png`), (await win.webContents.capturePage()).toPNG())
          }
        }
      }
    }
    const output = path.join(__dirname, '../tests/artist-ground-truth.json')
    fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n')
    console.log('EXTRACTED_ARTIST_GROUND_TRUTH_TO', output)
    win.destroy(); app.quit(); return
  }

  if (HOME_ONLY) {
    const folder = process.env.NCM_ELECTRON_SCREENSHOT_DIR || '/tmp/ncm-home-reference'
    fs.mkdirSync(folder, { recursive: true })
    fs.writeFileSync(path.join(folder, 'home-electron.png'), (await win.webContents.capturePage()).toPNG())
    fs.writeFileSync(path.join(folder, 'home-electron.json'), JSON.stringify(await win.webContents.executeJavaScript(`({
      title: __collectRectAndStyle('.page-title'), header: __collectRectAndStyle('.section-header'),
      card: __collectRectAndStyle('.large-card'), album: __collectRectAndStyle('.album-item')
    })`), null, 2))
    win.destroy(); app.quit(); return
  }

  if (FULLSCREEN_ONLY) {
    const run = (script) => win.webContents.executeJavaScript(script + '; true;')
    const screenshot = async (name) => {
      const dir = process.env.NCM_ELECTRON_SCREENSHOT_DIR
      if (!dir) return
      fs.mkdirSync(dir, { recursive: true })
      fs.writeFileSync(path.join(dir, name + '.png'), (await win.webContents.capturePage()).toPNG())
    }
    await run(`window.__player = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('player'); window.__playback = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('playback'); __player.isPlaying = false; __player.currentTime = 92000; __player.isFullScreen = true`)
    await wait(800)
    const capture = () => win.webContents.executeJavaScript(`(() => {
      const container = document.querySelector('.lyrics-scroll-container');
      const box = container.getBoundingClientRect();
      const rows = [...container.querySelectorAll('.lyric-line')];
      return { width: box.width, height: box.height, scrollTop: container.scrollTop,
        rows: rows.map(el => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el);
          return { text: el.querySelector('.lyric-text').textContent, top: r.y - box.y, height: r.height,
            layoutHeight: el.offsetHeight, center: r.y - box.y + r.height / 2,
            active: el.classList.contains('active'), opacity: s.opacity, transform: s.transform }; }) };
    })()`)
    const report = { initial: await capture() }
    if (report.initial.rows.length !== 5) throw new Error('Empty lyric markers created visible rows')
    await screenshot('fullscreen-electron-initial')
    const interaction = () => win.webContents.executeJavaScript(`(() => {
      const panel = document.querySelector('.lyrics-panel');
      const guide = panel.querySelector('.lyric-seek-guide');
      const button = panel.querySelector('.lyric-seek-play-btn');
      return {
        rows: [...panel.querySelectorAll('.lyric-line')].map(el => ({
          active: el.classList.contains('active'), centered: el.classList.contains('centered'),
          hovered: el.matches(':hover'), opacity: Number(getComputedStyle(el).opacity), title: el.title,
        })),
        indicator: !!button,
        time: panel.querySelector('.lyric-seek-time')?.textContent,
        guideStyle: guide && getComputedStyle(guide).borderTopStyle,
        buttonTitle: button?.title,
      };
    })()`)
    const hoverPoint = await win.webContents.executeJavaScript(`(() => {
      const r = document.querySelectorAll('.lyric-line')[2].getBoundingClientRect();
      return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) };
    })()`)
    win.webContents.sendInputEvent({ type: 'mouseMove', ...hoverPoint })
    await wait(220)
    report.interaction = { hover: await interaction() }
    win.webContents.sendInputEvent({ type: 'mouseWheel', ...hoverPoint, deltaY: 118, canScroll: true, hasPreciseScrollingDeltas: true })
    await wait(700)
    report.interaction.scrolled = await interaction()
    await screenshot('fullscreen-electron-hover-scroll')
    const centeredPoint = await win.webContents.executeJavaScript(`(() => {
      const r = document.querySelector('.lyric-line.centered').getBoundingClientRect();
      return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) };
    })()`)
    win.webContents.sendInputEvent({ type: 'mouseMove', ...centeredPoint })
    await wait(220)
    report.interaction.centeredHover = await interaction()
    win.webContents.sendInputEvent({ type: 'mouseMove', x: 10, y: 10 })
    await wait(220)
    report.interaction.centeredIdle = await interaction()
    await run(`__playback.isLoading = true`)
    await wait(250)
    report.interaction.loading = await interaction()
    if (report.interaction.loading.indicator) throw new Error('Loading state did not hide the seek indicator')
    await run(`__playback.isLoading = false; document.querySelector('.lyrics-panel').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))`)
    win.webContents.sendInputEvent({ type: 'mouseMove', x: 10, y: 10 })
    await wait(500)
    report.interaction.cancelled = await interaction()
    await run(`__player.currentTime = 107500`)
    await wait(450)
    await run(`__player.currentTime = 108000`)
    report.transition = []
    for (const delay of [0, 80, 100, 220, 400]) {
      await wait(delay)
      report.transition.push(await capture())
    }
    await screenshot('fullscreen-electron-follow')
    await run(`document.querySelectorAll('.lyric-toggle')[1].click()`)
    await wait(450)
    report.withoutTranslation = await capture()
    await screenshot('fullscreen-electron-without-translation')
    await run(`document.querySelectorAll('.lyric-toggle')[0].click()`)
    await wait(450)
    report.withoutExtras = await capture()
    await screenshot('fullscreen-electron-without-extras')
    fs.writeFileSync(path.join(__dirname, '../tests/fullscreen-ground-truth.json'), JSON.stringify(report, null, 2))
    win.destroy()
    app.quit()
    return
  }

  if (LOGIN_ONLY) {
    const report = {}
    const run = (script) => win.webContents.executeJavaScript(script + '; true;')
    const capture = () => win.webContents.executeJavaScript(`({
      card: __collectRectAndStyle('.login-card'),
      error: __collectRectAndStyle('.error-msg'),
      phone: __collectRectAndStyle('.phone-input-container'),
      secret: __collectRectAndStyle('.input-field'),
      submit: __collectRectAndStyle('.submit-btn'),
      codeButton: __collectRectAndStyle('.verify-btn'),
      remember: document.querySelector('.custom-checkbox input').checked,
      inputs: [...document.querySelectorAll('.main-form input:not([type=checkbox])')].map(i => i.value),
      errorText: document.querySelector('.error-msg')?.textContent || '',
      codeText: document.querySelector('.verify-btn')?.textContent.trim() || '',
      flipped: document.querySelector('.login-card').classList.contains('is-flipped'),
      qrStatus: document.querySelector('.qr-status-text').textContent,
    })`)
    await run(`document.querySelector('.menu-item.logout')?.click()`)
    await wait(100)
    await run(`document.querySelector('.profile-bubble').click()`)
    await wait(700)
    report.initial = await capture()
    await run(`window.__input = (sel,value) => {const i=document.querySelector(sel); i.value=value; i.dispatchEvent(new Event('input',{bubbles:true}));};
      __input('.prefix-selector input','44'); __input('#pwd-phone','13000000000'); __input('#pwd-input','test-password');
      document.querySelector('.main-form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`)
    await wait(250)
    report.passwordError = await capture()
    await run(`document.querySelectorAll('.segmented-control button')[1].click()`)
    await wait(700)
    report.smsInitial = await capture()
    await run(`document.querySelector('.verify-btn').click()`)
    await wait(200)
    report.smsMissingPhone = await capture()
    await run(`__input('.prefix-selector input','81'); __input('#code-phone','09000000000'); __input('#code-input','123456'); document.querySelector('.verify-btn').click()`)
    await wait(700)
    report.smsSent = await capture()
    await run(`document.querySelectorAll('.segmented-control button')[0].click()`)
    await wait(700)
    report.passwordRestored = await capture()
    await run(`document.querySelector('.qr-btn').click()`)
    await wait(700)
    report.qr = await capture()
    await run(`document.querySelector('.pc-btn').click()`)
    await wait(700)
    report.formRestored = await capture()
    await run(`document.querySelector('.face-front .close-btn').click()`)
    await wait(700)
    await run(`document.querySelector('.profile-bubble').click()`)
    await wait(700)
    report.reopened = await capture()
    // Freeze Chromium's actual rotateY animation at exact times. Corner probes
    // measure the perspective divide, rather than only the untransformed width.
    await run(`{
      const card = document.querySelector('.login-card');
      window.__flipProbes = [[0,0],[400,0],[400,560],[0,560]].map(([x,y]) => {
        const p = document.createElement('i');
        Object.assign(p.style, {position:'absolute',left:x+'px',top:y+'px',width:'0',height:'0'});
        card.appendChild(p); return p;
      });
      window.__flipAnimation = card.animate([{transform:'rotateY(0deg)'},{transform:'rotateY(180deg)'}],
        {duration:600,easing:'cubic-bezier(0.4, 0, 0.2, 1)',fill:'both'});
      __flipAnimation.pause();
    }`)
    report.flip = { durationMs: 600, perspective: 1200, samples: [], reverse: [] }
    for (const direction of ['samples', 'reverse']) {
    if (direction === 'reverse') {
      await run(`__flipAnimation.effect.setKeyframes([{transform:'rotateY(180deg)'},{transform:'rotateY(0deg)'}])`)
    }
    for (const tMs of [0,60,120,180,210,240,300,450,600]) {
      await run(`__flipAnimation.currentTime = ${tMs}`)
      const sample = await win.webContents.executeJavaScript(`({
        tMs: ${tMs}, transform:getComputedStyle(document.querySelector('.login-card')).transform,
        rect:__collectRectAndStyle('.login-card').rect,
        corners:__flipProbes.map(p => {const r=p.getBoundingClientRect();return {x:r.x,y:r.y}})
      })`)
      report.flip[direction].push(sample)
      if (process.env.NCM_ELECTRON_SCREENSHOT_DIR) {
        await win.webContents.executeJavaScript('new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))')
        const folder = process.env.NCM_ELECTRON_SCREENSHOT_DIR
        fs.mkdirSync(folder, { recursive: true })
        fs.writeFileSync(path.join(folder, `login-flip-${direction === 'reverse' ? 'reverse-' : ''}${tMs}.png`), (await win.webContents.capturePage()).toPNG())
      }
    }
    }
    await run(`__flipAnimation.cancel(); __flipProbes.forEach(p=>p.remove())`)
    fs.writeFileSync(path.join(__dirname,'../tests/login-modal-ground-truth.json'), JSON.stringify(report,null,2))
    console.log('Login behavior and geometry captured (mock IPC only)')
    app.quit()
    return
  }

  if (HOVER_ONLY) {
    const report = {}
    const capture = async () => win.webContents.executeJavaScript(`Object.fromEntries(
      Object.entries({bubble: '.profile-bubble', trigger: '.bubble-trigger', menu: '.bubble-menu', user: '.user-section', content: '.sidebar-content'})
        .map(([key, selector]) => [key, __collectRectAndStyle(selector)]))`)
    report.closed = await capture()
    win.webContents.sendInputEvent({ type: 'mouseMove', x: 120, y: 940 })
    await wait(500)
    report.open = await capture()
    if (process.env.NCM_ELECTRON_SCREENSHOT_DIR) {
      fs.mkdirSync(process.env.NCM_ELECTRON_SCREENSHOT_DIR, { recursive: true })
      fs.writeFileSync(path.join(process.env.NCM_ELECTRON_SCREENSHOT_DIR, 'sidebar-hover.png'), (await win.webContents.capturePage()).toPNG())
    }
    const output = path.join(__dirname, '../tests/sidebar-hover-ground-truth.json')
    fs.writeFileSync(output, JSON.stringify(report, null, 2))
    console.log('EXTRACTED_SIDEBAR_GROUND_TRUTH_TO', output)
    win.destroy()
    app.quit()
    return
  }

  if (SETTINGS_ONLY) {
    const report = {}
    const selectors = {
      window: '.settings-window', sidebar: '.settings-sidebar',
      navIndicator: '.nav-indicator', navItem: '.settings-sidebar .nav-item',
      mainHeader: '.main-header', closeBtn: '.main-header .close-btn',
      contentScroll: '.content-scroll', controlCard: '.control-card',
      controlRow: '.control-row', modernSwitch: '.modern-switch'
    }
    await win.webContents.executeJavaScript(`window.location.hash = '#/settings'; true;`)
    for (const [width, height] of [[1000, 800], [1200, 900]]) {
      win.setContentSize(width, height)
      await wait(650)
      const view = `${width}x${height}`
      report[view] = await win.webContents.executeJavaScript(`Object.fromEntries(
        Object.entries(${JSON.stringify(selectors)}).map(([key, selector]) =>
          [key, __collectRectAndStyle(selector)]))`)
      for (const [key, value] of Object.entries(report[view])) {
        if (!value) throw new Error(`Missing element settings.${view}.${key}: ${selectors[key]}`)
      }
      if (process.env.NCM_ELECTRON_SCREENSHOT_DIR) {
        fs.mkdirSync(process.env.NCM_ELECTRON_SCREENSHOT_DIR, { recursive: true })
        const screenshot = await win.webContents.capturePage()
        fs.writeFileSync(path.join(process.env.NCM_ELECTRON_SCREENSHOT_DIR, `settings-${view}.png`), screenshot.toPNG())
      }
    }
    // Capture the contents, not just the outer settings shell. Keep coordinates
    // relative to the scroll content so below-the-fold rows are also testable.
    report.tabs = {}
    const tabs = ['general', 'audio', 'appearance', 'library', 'cache', 'shortcuts', 'debug', 'about']
    for (const [width, height] of [[1000, 800], [1200, 900]]) {
      win.setContentSize(width, height)
      const view = `${width}x${height}`
      report.tabs[view] = {}
      for (const [index, tab] of tabs.entries()) {
        await win.webContents.executeJavaScript(`document.querySelectorAll('.settings-sidebar .nav-item')[${index}].click(); document.querySelector('.content-scroll').scrollTop = 0; true;`)
        await wait(1200)
        report.tabs[view][tab] = await win.webContents.executeJavaScript(`(() => {
          const rect = el => { const r = el.getBoundingClientRect(); return {x: r.x, y: r.y, width: r.width, height: r.height} }
          const all = selector => Array.from(document.querySelectorAll(selector), rect)
          return {rows: all('.control-row'), titles: all('.control-row .primary'), descriptions: all('.control-row .secondary'), actions: all('.control-row .action'), cards: all('.control-card'), groups: all('.setting-group'), tips: all('.tip-text'), cache: all('.cache-card'), cacheItems: all('.cache-breakdown-item'), paths: all('.settings-path-item'), scrollHeight: document.querySelector('.content-scroll').scrollHeight}
        })()`)
        if (process.env.NCM_ELECTRON_SCREENSHOT_DIR) {
          fs.writeFileSync(path.join(process.env.NCM_ELECTRON_SCREENSHOT_DIR, `settings-${view}-${tab}.png`), (await win.webContents.capturePage()).toPNG())
        }
      }
    }
    const output = path.join(__dirname, '../tests/settings-window-ground-truth.json')
    fs.writeFileSync(output, JSON.stringify(report, null, 2))
    console.log('EXTRACTED_SETTINGS_GROUND_TRUTH_TO', output)
    win.destroy()
    app.quit()
    return
  }

  if (PLAYLIST_ONLY) {
    const report = {}
    const collect = async (view, selectors) => {
      report[view] = await win.webContents.executeJavaScript(`Object.fromEntries(
        Object.entries(${JSON.stringify(selectors)}).map(([key, selector]) => {
          const value = __collectRectAndStyle(selector);
          return [key, value];
        }))`)
      for (const [key, value] of Object.entries(report[view])) {
        if (!value) throw new Error(`Missing element ${view}.${key}: ${selectors[key]}`)
      }
      if (process.env.NCM_ELECTRON_SCREENSHOT_DIR) {
        await wait(50)
        fs.mkdirSync(process.env.NCM_ELECTRON_SCREENSHOT_DIR, { recursive: true })
        const screenshot = await win.webContents.capturePage()
        fs.writeFileSync(path.join(process.env.NCM_ELECTRON_SCREENSHOT_DIR, `${view}.png`), screenshot.toPNG())
      }
    }
    await collect('sidebar', { header: '.nav-group-header', title: '.nav-group-title', create: '.nav-group-action' })
    await win.webContents.executeJavaScript(`document.querySelector('.nav-group-action').click(); true;`)
    await wait(100)
    const createSelectors = {
      card: '.create-playlist-card', header: '.create-playlist-card .card-header',
      close: '.create-playlist-card .close-btn', name: '.name-input', privacy: '.privacy-row',
      toggle: '.privacy-row .switch', cancel: '.create-playlist-card .btn-secondary',
      submit: '.create-playlist-card .btn-primary'
    }
    await collect('create', createSelectors)
    await win.webContents.executeJavaScript(`document.querySelector('.create-playlist-card .close-btn').click(); window.location.hash = '#/playlist/101'; true;`)
    await wait(300)
    await collect('detail', { play: '.action-bar .play-main-btn', search: '.search-box-container', subscribers: '.subscriber-count', more: '.menu-trigger' })
    await win.webContents.executeJavaScript(`document.querySelector('.menu-trigger').click(); true;`)
    await wait(150)
    await collect('menu', { card: '.action-menu', edit: '.action-menu button', divider: '.action-menu .menu-divider', delete: '.delete-action' })
    await win.webContents.executeJavaScript(`document.querySelector('.action-menu button').click(); true;`)
    await wait(100)
    const editSelectors = {
      card: '.edit-playlist-dialog', header: '.edit-playlist-card .card-header',
      close: '.edit-playlist-card .close-btn', cover: '.preview-cover',
      name: '.edit-playlist-card input', description: '.edit-playlist-card textarea',
      footer: '.edit-playlist-card .card-actions', cancel: '.edit-playlist-card .btn-secondary',
      submit: '.edit-playlist-card .btn-primary'
    }
    await collect('edit', editSelectors)
    win.setContentSize(620, 720)
    await wait(300)
    await collect('editCompact', editSelectors)
    await win.webContents.executeJavaScript(`document.querySelector('.edit-playlist-card .close-btn').click(); true;`)
    await collect('detailCompact', { header: '.playlist-header', cover: '.playlist-header .cover-wrapper', title: '.playlist-title', actions: '.action-bar', play: '.action-bar .play-main-btn', search: '.search-box-container', subscribers: '.subscriber-count', more: '.menu-trigger' })
    await win.webContents.executeJavaScript(`window.location.hash = '#/home'; true;`)
    await wait(200)
    await collect('sidebarCompact', { header: '.nav-group-header', create: '.nav-group-action' })
    await win.webContents.executeJavaScript(`document.querySelector('.nav-group-action').click(); true;`)
    await wait(100)
    await collect('createCompact', createSelectors)
    const output = path.join(__dirname, '../tests/playlist-management-ground-truth.json')
    fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n')
    console.log('EXTRACTED_PLAYLIST_GROUND_TRUTH_TO', output)
    app.exit(0)
    return
  }

  const report = {
    viewport: { width: 1435, height: 1000 },
    views: {},
    responsive: {},
    animations: {}
  }

  // Sample real Chromium Web Animation curves for every transition & keyframe in the app
  report.animations = await win.webContents.executeJavaScript(`({
    modernSwitchKnob: __sampleChromiumCurve(300, 'cubic-bezier(0.2, 1, 0.3, 1)', 2, 22),
    segmentedSliderSpring: __sampleChromiumCurve(400, 'cubic-bezier(0.18, 0.89, 0.32, 1.1)', 0, 100),
    settingsNavIndicator: __sampleChromiumCurve(400, 'cubic-bezier(0.2, 1, 0.3, 1)', 82.59, 186.59),
    settingsNavIconScale: __sampleChromiumCurve(300, 'cubic-bezier(0.34, 1.56, 0.64, 1)', 18, 20.7),
    profileBubbleExpand: __sampleChromiumCurve(400, 'cubic-bezier(0.165, 0.84, 0.44, 1)', 56, 181),
    profileBubbleLift: __sampleChromiumCurve(400, 'cubic-bezier(0.165, 0.84, 0.44, 1)', 0, -5),
    playlistOverlayEnterY: __sampleChromiumCurve(400, 'cubic-bezier(0.16, 1, 0.3, 1)', 368, 348),
    playlistOverlayEnterOpacity: __sampleChromiumCurve(400, 'cubic-bezier(0.16, 1, 0.3, 1)', 0, 1),
    volumePopupEnterY: __sampleChromiumCurve(200, 'ease', 858.5, 852.5),
    mediaCardHoverLift: __sampleChromiumCurve(250, 'ease', 0, -4),
    albumItemHoverLift: __sampleChromiumCurve(300, 'ease', 0, -4),
    albumItemCoverZoom: __sampleChromiumCurve(350, 'ease', 176, 186.56),
    searchTitleCollapse: __sampleChromiumCurve(300, 'ease', 54.39, 0),
    searchBarFloatingWidth: __sampleChromiumCurve(400, 'ease', 600, 500),
    detailSearchFocusWidth: __sampleChromiumCurve(300, 'ease', 220, 260),
    artistTileHoverLift: __sampleChromiumCurve(300, 'cubic-bezier(0.25, 1, 0.5, 1)', 0, -4),
    fullscreenFadeEnterY: __sampleChromiumCurve(350, 'ease', 20, 0),
    fullscreenCoverScale: __sampleChromiumCurve(800, 'cubic-bezier(0.2, 0, 0.2, 1)', 459, 450),
    lyricLineActiveOpacity: __sampleChromiumCurve(150, 'cubic-bezier(0.23, 1, 0.32, 1)', 0.3, 1.0),
    lyricLineActiveHeight: __sampleChromiumCurve(350, 'cubic-bezier(0.23, 1, 0.32, 1)', 106.23, 116.86),
    loginSegmentSlide: __sampleChromiumCurve(300, 'cubic-bezier(0.4, 0, 0.2, 1)', 554.5, 717.5),
    loginCardFlip: __sampleChromiumCurve(600, 'cubic-bezier(0.4, 0, 0.2, 1)', 0, 1),
    settingsAppleScaleUpY: __sampleChromiumCurve(500, 'cubic-bezier(0.2, 0.8, 0.2, 1)', 16, 0),
    appDialogFadeOpacity: __sampleChromiumCurve(180, 'ease', 0, 1)
  })`)

  // 1. Home View (logged in)
  report.views.home = await win.webContents.executeJavaScript(`({
    sidebar: __collectRectAndStyle('.sidebar-floating'),
    sidebarContent: __collectRectAndStyle('.sidebar-content'),
    navItem: __collectRectAndStyle('.nav-item'),
    navItemActive: __collectRectAndStyle('.nav-item.active'),
    navIcon: __collectRectAndStyle('.nav-icon'),
    navGroupTitle: __collectRectAndStyle('.nav-group-title'),
    profileBubble: __collectRectAndStyle('.profile-bubble'),
    bubbleTrigger: __collectRectAndStyle('.bubble-trigger'),
    avatarWrapper: __collectRectAndStyle('.avatar-wrapper'),
    playerBar: __collectRectAndStyle('.player-bar.glass-morphism'),
    playerCover: __collectRectAndStyle('.player-bar .cover-wrapper'),
    playMainBtn: __collectRectAndStyle('.player-bar .play-main-btn'),
    progressTrack: __collectRectAndStyle('.player-bar .progress-track'),
    pageTitle: __collectRectAndStyle('.page-title'),
    sectionHeaderH2: __collectRectAndStyle('.section-header h2'),
    mediaCard: __collectRectAndStyle('.large-card'),
    mediaCardPlayBtn: __collectRectAndStyle('.play-btn-circle'),
    albumItem: __collectRectAndStyle('.album-item'),
    albumCover: __collectRectAndStyle('.album-cover')
  })`)

  // 2. PlaylistOverlay (wait 450ms for 400ms enter transition to finish)
  await win.webContents.executeJavaScript(`document.querySelector('.list-btn')?.click(); true;`)
  await wait(450)
  report.views.playlistOverlay = await win.webContents.executeJavaScript(`({
    card: __collectRectAndStyle('.playlist-card'),
    header: __collectRectAndStyle('.playlist-header'),
    title: __collectRectAndStyle('.playlist-header .title'),
    clearBtn: __collectRectAndStyle('.playlist-header .clear-btn'),
    activeItem: __collectRectAndStyle('.playlist-item.active'),
    normalItem: __collectRectAndStyle('.playlist-item:not(.active)'),
    itemCover: __collectRectAndStyle('.playlist-item .item-cover'),
    playingBar: __collectRectAndStyle('.playing-icon .bar')
  })`)
  await win.webContents.executeJavaScript(`document.querySelector('.list-btn')?.click(); true;`)
  await wait(350)

  // 3. VolumePopup on PlayerBar (wait 250ms for 200ms enter transition)
  await win.webContents.executeJavaScript(`document.querySelector('.volume-btn')?.click(); true;`)
  await wait(250)
  report.views.volumePopup = await win.webContents.executeJavaScript(`({
    popup: __collectRectAndStyle('.volume-popup'),
    slider: __collectRectAndStyle('.volume-popup .volume-slider'),
    value: __collectRectAndStyle('.volume-popup .volume-value'),
    track: __collectRectAndStyle('.volume-popup .custom-slider-track')
  })`)
  await win.webContents.executeJavaScript(`document.querySelector('.volume-btn')?.click(); true;`)
  await wait(250)

  // 4. AddToPlaylistModal on PlayerBar
  await win.webContents.executeJavaScript(`document.querySelector('.player-bar .more-btn')?.click(); true;`)
  await wait(250)
  report.views.addToPlaylistModal = await win.webContents.executeJavaScript(`({
    card: __collectRectAndStyle('.add-playlist-card'),
    header: __collectRectAndStyle('.add-playlist-card .card-header'),
    closeBtn: __collectRectAndStyle('.add-playlist-card .close-btn'),
    playlistItem: __collectRectAndStyle('.add-playlist-card .playlist-item'),
    playlistCover: __collectRectAndStyle('.add-playlist-card .playlist-cover')
  })`)
  await win.webContents.executeJavaScript(`document.querySelector('.add-playlist-card .close-btn')?.click(); true;`)
  await wait(150)

  // 5. Search View
  await win.webContents.executeJavaScript(`window.location.hash = '#/search'; true;`)
  await wait(300)
  report.views.search = await win.webContents.executeJavaScript(`({
    pageTitle: __collectRectAndStyle('.search-header .page-title'),
    searchBarWrapper: __collectRectAndStyle('.search-bar-wrapper'),
    searchInput: __collectRectAndStyle('.search-input'),
    categoryCard: __collectRectAndStyle('.category-card'),
    categoryName: __collectRectAndStyle('.category-name'),
    cardDecoration: __collectRectAndStyle('.card-decoration')
  })`)

  // 6. Playlist Detail View + SongContextMenu
  await win.webContents.executeJavaScript(`window.location.hash = '#/playlist/101'; true;`)
  await wait(300)
  await win.webContents.executeJavaScript(`document.querySelector('.track-row .row-more')?.click(); true;`)
  await wait(150)
  report.views.playlistDetail = await win.webContents.executeJavaScript(`({
    container: __collectRectAndStyle('.playlist-container'),
    header: __collectRectAndStyle('.playlist-header'),
    coverWrapper: __collectRectAndStyle('.playlist-header .cover-wrapper'),
    title: __collectRectAndStyle('.playlist-title'),
    playMainBtn: __collectRectAndStyle('.action-bar .play-main-btn'),
    searchBox: __collectRectAndStyle('.search-box-container'),
    secondaryBtn: __collectRectAndStyle('.secondary-btn'),
    moreBtn: __collectRectAndStyle('.action-bar .icon-btn'),
    listHeader: __collectRectAndStyle('.list-header-content'),
    trackRow: __collectRectAndStyle('.track-row'),
    miniCover: __collectRectAndStyle('.mini-cover-wrapper'),
    contextMenu: __collectRectAndStyle('.song-context-menu'),
    contextMenuItem: __collectRectAndStyle('.song-context-menu .menu-item')
  })`)

  // 7. Local Music View + AppDialog
  await win.webContents.executeJavaScript(`window.location.hash = '#/local-music'; true;`)
  await wait(250)
  await win.webContents.executeJavaScript(`document.querySelector('.local-icon-button')?.click(); true;`)
  await wait(220)
  report.views.localMusic = await win.webContents.executeJavaScript(`({
    cover: __collectRectAndStyle('.local-library-cover'),
    importBtn: __collectRectAndStyle('.import-button'),
    clearIconBtn: __collectRectAndStyle('.local-icon-button'),
    appDialog: __collectRectAndStyle('.app-dialog'),
    dialogPrimaryBtn: __collectRectAndStyle('.dialog-btn-primary'),
    dialogSecondaryBtn: __collectRectAndStyle('.dialog-btn-secondary')
  })`)
  await win.webContents.executeJavaScript(`document.querySelector('.dialog-btn-secondary')?.click(); true;`)
  await wait(200)

  // 8. Artist View
  await win.webContents.executeJavaScript(`window.location.hash = '#/artist/100'; true;`)
  await wait(350)
  report.views.artist = await win.webContents.executeJavaScript(`({
    mainContent: __collectRectAndStyle('.main-content'),
    header: __collectRectAndStyle('.artist-header'),
    avatarCol: __collectRectAndStyle('.artist-avatar-col'),
    artistName: __collectRectAndStyle('.artist-name'),
    pill: __collectRectAndStyle('.pill'),
    btnPlay: __collectRectAndStyle('.btn-play'),
    btnShuffle: __collectRectAndStyle('.btn-shuffle'),
    latestItem: __collectRectAndStyle('.latest-item'),
    latestImg: __collectRectAndStyle('.latest-img-wrap-wrapper'),
    songsCard: __collectRectAndStyle('.songs-card'),
    artistSongRow: __collectRectAndStyle('.songs-card .song-row'),
    artistSongThumb: __collectRectAndStyle('.song-thumb-wrapper'),
    songBadgeHot: __collectRectAndStyle('.badge-hot'),
    mediaTileImg: __collectRectAndStyle('.tile-img-wrap-wrapper'),
    mvTileImg: __collectRectAndStyle('.mv-img-wrap-wrapper'),
    mvPlayBtn: __collectRectAndStyle('.mv-play-btn')
  })`)

  // 9. Fullscreen Player + LyricPanel + CommentPanel
  await win.webContents.executeJavaScript(`document.querySelector('.player-bar .section-left')?.click(); true;`)
  await wait(450)
  report.views.fullscreen = await win.webContents.executeJavaScript(`({
    playerContent: __collectRectAndStyle('.player-content'),
    backBtn: __collectRectAndStyle('.back-btn'),
    mainLayout: __collectRectAndStyle('.main-layout'),
    coverWrapper: __collectRectAndStyle('.visual-panel .cover-wrapper'),
    trackTitle: __collectRectAndStyle('.visual-panel .track-title'),
    progressBarBg: __collectRectAndStyle('.progress-bar-bg'),
    playMainBtn: __collectRectAndStyle('.btns-transport .play-main-btn'),
    lyricLine: __collectRectAndStyle('.lyric-line:not(.active)'),
    lyricLineActive: __collectRectAndStyle('.lyric-line.active'),
    lyricToggle: __collectRectAndStyle('.lyric-toggle')
  })`)

  await win.webContents.executeJavaScript(`document.querySelectorAll('.btns-side .aux-btn')[1]?.click(); true;`)
  await wait(250)
  report.views.commentPanel = await win.webContents.executeJavaScript(`({
    sectionLabel: __collectRectAndStyle('.section-label'),
    commentItem: __collectRectAndStyle('.comment-item'),
    commentAvatar: __collectRectAndStyle('.comment-avatar'),
    commentReply: __collectRectAndStyle('.comment-reply')
  })`)

  await win.webContents.executeJavaScript(`document.querySelector('.back-btn')?.click(); true;`)
  await wait(400)

  // 10. LoginModal (logout first, then click profile-bubble)
  await win.webContents.executeJavaScript(`window.location.hash = '#/home'; true;`)
  await wait(200)
  await win.webContents.executeJavaScript(`document.querySelector('.menu-item.logout')?.click(); true;`)
  await wait(150)
  await win.webContents.executeJavaScript(`document.querySelector('.profile-bubble')?.click(); true;`)
  await wait(350)
  report.views.loginModal = await win.webContents.executeJavaScript(`({
    card: __collectRectAndStyle('.login-card'),
    faceFront: __collectRectAndStyle('.face-front'),
    qrBtn: __collectRectAndStyle('.qr-btn'),
    closeBtn: __collectRectAndStyle('.face-front .close-btn'),
    segmentedControl: __collectRectAndStyle('.segmented-control'),
    segmentBg: __collectRectAndStyle('.segment-bg'),
    phoneInputContainer: __collectRectAndStyle('.phone-input-container'),
    inputField: __collectRectAndStyle('.input-field'),
    checkmark: __collectRectAndStyle('.checkmark'),
    submitBtn: __collectRectAndStyle('.submit-btn'),
    qrBox: __collectRectAndStyle('.qr-box')
  })`)

  // 11. Settings Modal (wait 580ms for 500ms appleScaleUp to complete)
  await win.webContents.executeJavaScript(`window.location.hash = '#/settings'; true;`)
  await wait(580)
  report.views.settings = await win.webContents.executeJavaScript(`({
    window: __collectRectAndStyle('.settings-window'),
    sidebar: __collectRectAndStyle('.settings-sidebar'),
    navIndicator: __collectRectAndStyle('.nav-indicator'),
    navItem: __collectRectAndStyle('.settings-sidebar .nav-item'),
    mainHeader: __collectRectAndStyle('.main-header'),
    mainHeaderH2: __collectRectAndStyle('.main-header h2'),
    closeBtn: __collectRectAndStyle('.main-header .close-btn'),
    contentScroll: __collectRectAndStyle('.content-scroll'),
    controlCard: __collectRectAndStyle('.control-card'),
    controlRow: __collectRectAndStyle('.control-row'),
    modernSwitch: __collectRectAndStyle('.modern-switch')
  })`)

  // 12. Responsive Breakpoints (@media <= 1000px and <= 650px)
  await win.webContents.executeJavaScript(`window.location.hash = '#/home'; true;`)
  await wait(200)
  win.setContentSize(950, 800)
  await wait(250)
  report.responsive.w950 = await win.webContents.executeJavaScript(`({
    sidebar: __collectRectAndStyle('.sidebar-floating'),
    playerBar: __collectRectAndStyle('.player-bar.glass-morphism')
  })`)

  win.setContentSize(620, 720)
  await wait(250)
  report.responsive.w620 = await win.webContents.executeJavaScript(`({
    sidebar: __collectRectAndStyle('.sidebar-floating'),
    playerBar: __collectRectAndStyle('.player-bar.glass-morphism')
  })`)

  fs.writeFileSync(OUTPUT_JSON, JSON.stringify(report, null, 2))
  console.log('EXTRACTED_GROUND_TRUTH_TO', OUTPUT_JSON)
  app.exit(0)
}).catch((error) => {
  console.error(error)
  app.exit(1)
})
