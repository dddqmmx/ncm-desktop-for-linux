# 歌手预览页

Slint 歌手页按 `src/renderer/src/views/ArtistView.vue` 的内容布局：主容器最大 1100px，左右内边距 32px，顶部 56px，底部 120px；区块相距 60px。头像 200px，头像与正文间隔 48px，标题随窗口宽度变化，简介保留全部文字并自动换行。最新发布使用两列；热门歌曲按实际条目数计算高度；专辑、EP 和 MV 使用自动换行的网格（最小列宽分别为 180px、180px、260px，间距 24px），每行按最长的两行标题计算高度。空区块不占位。

加载期间隐藏旧歌手内容；切换歌手或重试时回到页首，失败可重试。数据沿用 Electron 的发行时间排序、type/subType 分类、数量标签、简介回退、头像和宽幅 MV 图片优先级、热门歌曲前两项标记。年份按本地时区显示。

`artist-ground-truth.json` 来自当前 Electron DOM，覆盖 1340、1435、1800 宽度、短/长简介、50 首歌曲、多行专辑/EP/MV 和空数据。测试比较头部、各区块、全部歌曲行及封面的位置/尺寸，容差 1 个逻辑像素，并检查滚动能到达专辑网格。软件及 Vulkan 截图还检查长标题的两行均有实际文字像素，防止留了两行高度却把首行或第二行误裁切。另有请求失败、重试、旧响应隔离、专辑分类和完整 ID 的运行时回归。原 `electron-ground-truth.json` 中的历史歌手静态尺寸不再用于页面布局断言，悬停动画仍使用 Chromium 曲线回归。

重新采集（从仓库根目录运行，使用模拟 IPC）：

```sh
(cd electron && npm run build)
NCM_ELECTRON_SCREENSHOT_DIR=/tmp/ncm-artist-reference electron/node_modules/.bin/electron --no-sandbox slint-ui/scripts/extract-electron-metrics.cjs --artist-view
```

在 `slint-ui` 目录运行回归及可选截图：

```sh
NCM_SLINT_SCREENSHOT_DIR=/tmp/ncm-artist-review cargo test --locked artist_ -- --nocapture
NCM_SLINT_SCREENSHOT_DIR=/tmp/ncm-artist-real timeout 60s cargo test --locked real_artist_page_scrolls_through_songs_albums_and_mvs -- --ignored --nocapture
```

真实窗口测试默认使用与应用相同的 Vulkan 渲染器，展示长简介、滚动至专辑和 MV，再检查加载时回到页首。测试图片为统一的本地色块，不访问真实账号。几何通过不表示字体栅格化、玻璃背景和阴影逐像素相同；软件渲染的圆角裁剪效果也与 Vulkan 有差异。

本次验证：`cargo test --locked` 49 项通过、11 项需手动环境的测试默认跳过；上述真实 Vulkan 歌手页测试单独运行通过。
