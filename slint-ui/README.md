# Slint 应用

默认启动真实 Rust 运行时；`--demo` 保留原来的离线演示和像素回归数据。

```sh
npm run dev:slint
# 或
cargo run --manifest-path slint-ui/Cargo.toml --locked
# 离线演示，不启动 API / native 运行时，不读取真实会话
cargo run --manifest-path slint-ui/Cargo.toml --locked -- --demo
```

默认构建包含 Vulkan、OpenGL 和软件渲染。首次运行及旧配置缺少渲染字段时使用 Vulkan，不再需要额外的启动参数。在「设置 → 通用 → 界面渲染」中使用通用下拉菜单选择：

- Vulkan（默认）：Skia Vulkan 渲染。
- OpenGL：FemtoVG OpenGL 渲染。
- 软件渲染（关闭 GPU 加速）：Slint 软件渲染器在 CPU 上绘制界面。

选择保存到 `preferences.json` 的 `renderer` 字段，值分别为 `vulkan`、`opengl`、`software`。修改后重启应用生效；启动在创建任何窗口之前读取此字段，主窗口、设置窗口和登录动画的离屏窗口共用该渲染方式。刚修改就退出也会在关闭运行时前提交保存。示例：`"renderer":"software"` 可在无法打开界面时手动关闭应用自身的 GPU 渲染；系统桌面合成器的渲染方式由操作系统管理。

在 `slint-ui` 目录中可省略 `--manifest-path slint-ui/Cargo.toml`。Vulkan 首次构建需要下载 Skia 原生库；使用 Vulkan 需要可用的 Vulkan 驱动。后端选择集中在 `src/rendering.rs`，真实窗口回归使用同一配置，软件布局测试不依赖显卡。渲染器更换不能替代列表绑定、布局和图片处理的性能优化。

设备确认、真实窗口检查和滚动测量见 [Vulkan 渲染验证](tests/vulkan-rendering.md)。

当前 Node 方案需要安装项目的 npm 依赖并能找到 `node`。Slint 不启动 Electron；播放、缓存、下载、文件扫描和配置存储直接在 Rust 执行。API 默认使用可替换的 Node 子进程，也可通过 `NCM_API_URL` 接入 v1 HTTP 提供方。接口、参数及纯 Rust 替换方式见 [API.md](API.md)。

已经接入：手机密码/验证码登录、二维码登录/轮询/过期刷新、保持登录和退出；推荐、搜索分页、喜欢的歌曲、歌单全量曲目、歌手/专辑/MV、歌词/译文与评论；云端歌单创建/编辑/删除/增删曲目；播放队列、暂停/继续、上一首/下一首、随机/单曲循环、进度拖动和歌词跳转；真实输出设备、软件音量、独占/BitPerfect；本地文件夹扫描/移除；native 缓存容量、预下载上限和清理；下载权限及音质选择、原生保存窗口和完整性检查。MV 交给系统外部处理程序打开。

请求、文件扫描与播放启动在后台执行。失败会显示实际状态，不会伪造已登录、已收藏或已播放；歌单写入失败保留草稿。歌曲 ID 使用完整字符串，不受 Slint int 的 32 位范围限制。

UI 主线程只提交运行时命令、接收结果并更新模型，禁止发起网络请求或解码远程图片。封面读取（包括缓存命中）、解码、缩放以及登录二维码解码均在后台阻塞任务中执行，最多同时解码两张图。传入 UI 的像素缓冲队列最多八张，每次更新最多处理四张；小列表封面最长边为 128 像素；首页卡片按显示尺寸、屏幕缩放和悬停放大选择 256 / 384 / 512 / 800 / 1600 像素档位，详情为 800 像素，全屏播放器按需请求 1600 像素，保持比例且不放大小尺寸源图。网易云图片 CDN 使用对应尺寸的缩略图参数，其他提供方的 URL 保持原样。封面到达时通过 ID 索引只更新对应模型行，不遍历整份歌单；晚到的低分辨率图片不会覆盖已有高清图。解码图片缓存最多 1024 张、64 MiB，淘汰时释放模型里的旧图片引用；当前播放及页面头图保留，回访已缓存卡片直接复用图片，超过预算被淘汰后才从磁盘缓存重新加载。

歌单、专辑、喜欢的音乐、本地音乐、搜索结果和侧栏歌单按窗口高度保留固定数量的行控件，循环复用，仅在新歌曲进入视口时替换对应行的数据，保留完整滚动高度与数据模型。滚动位置不直接传递给行内容绑定，避免整屏反复测量文字与重建封面纹理；模型内容变化仍会更新对应行。待播清单使用虚拟列表，关闭动画结束后释放列表行。添加到歌单弹窗与子菜单也复用固定数量的行，子菜单最多显示 240px 内容。全屏播放器关闭后释放内部内容。列表封面在对应项进入窗口可见范围时请求，取消整页图片预加载；下载并发上限八个，按 URL、尺寸和会话对进行中的请求去重。

歌单切换会立即更新侧栏选中项、已知标题和封面，并回到页首。尚未返回的基本信息与歌曲列表使用和主页相同的扫光占位；基本信息先展示，完整曲目继续后台加载。快速切换或离开页面后忽略旧请求的基本信息、歌曲及错误，旧封面不会覆盖新歌单。失败显示具体错误并支持重新加载，只有成功返回的空歌单才显示“暂无歌曲”。

主窗口隐藏系统标题栏，支持顶部空白区拖动、边缘缩放，右上角提供最小化、最大化/还原和关闭按钮。默认 1435×1000，最小 1340×960；独立设置窗口默认及最小尺寸为 1000×800，均可继续放大。设置窗口可拖动、调整尺寸；重复打开聚焦现有窗口，Esc/关闭只关闭设置，关闭主窗口会一并结束运行时。设置在两个窗口之间按字段同步，偏好会保存到磁盘。

音质和输出设备使用通用 [Dropdown](ui/components/dropdown.slint) 下拉组件。调用方提供 `option-count`、`option-label(index)` 和 `current-index`，在 `selected(index)` 中提交选择；组件不依赖业务状态。支持当前项勾选、方向键/Home/End 浏览、回车确认、Esc/点击外部取消，长菜单按可见范围创建选项。可用 `enabled`、`placeholder`、`popup-width` 和 `max-visible-items` 调整行为。七档音质的值和名称定义在 `AppState.sound-quality-options`，选择后沿用偏好持久化，重启从保存值恢复名称。

## 配置与数据

| 变量 | 用途 |
| --- | --- |
| `NCM_NODE` | Node 可执行文件，默认 `node` |
| `NCM_API_SIDECAR` | 自定义本地 API 适配脚本 |
| `NCM_API_URL` | v1 HTTP 提供方完整地址；设置后不启动 Node |
| `NCM_DATA_DIR` | 覆盖偏好/会话/本地曲库目录 |
| `NCM_CACHE_DIR` | 覆盖 native 缓存目录 |

默认配置目录为 `$XDG_CONFIG_HOME/ncm-slint`（回退 `~/.config/ncm-slint`），缓存为 `$XDG_CACHE_HOME/ncm-slint`（回退 `~/.cache/ncm-slint`）。独立于 Electron 数据，不自动导入旧登录信息。

`preferences.json` 保存音频、主题和文件夹等偏好，`library.json` 保存本地曲库索引；扫码登录默认将会话写入 `session.json`，密码/验证码登录则在勾选保持登录时保存。配置采用原子替换；Linux 文件权限为 0600，配置目录为 0700。会话文件使用本机文件权限保护，没有接入系统密钥环；密码不落盘。退出登录或使用未勾选保持登录的密码/验证码登录会删除旧会话文件。

登录弹窗默认打开密码页，区号为 86，表单的保持登录默认不勾选。密码与验证码页分别保留输入；切换标签或扫码模式清除错误，关闭重开重置表单。支持编辑区号、回车提交、点击整个保持登录标签；验证码发送成功后倒计时 60 秒。二维码自动加载、轮询和过期刷新，切回表单或关闭时取消轮询并忽略旧结果。扫码登录默认保持登录，不受表单勾选状态影响，重启后自动恢复会话。登录成功立即关闭弹窗，用户资料在后台加载。

卡片翻转保持 400×560 的布局，以 600ms、`cubic-bezier(0.4, 0, 0.2, 1)` 和 1200px 透视投影整张卡片。正反面内容先在隐藏窗口中渲染为像素缓冲，再分割成 2px 条带投影，文字、输入框、二维码和阴影一起变形；纹理上传总量仅为两张卡片，条带边缘按物理像素对齐避免接缝。动画过程中不逐帧截图或解码图片。支持翻转途中点击切换/关闭，重新打开从表单静止状态开始。逐帧几何对照 Chromium 的双向旋转基准，卡片边缘误差限制为 0.65 个逻辑像素；条带仍是透视近似，玻璃模糊及字体栅格化的渲染差异不在此保证内。

扫描跳过隐藏文件和符号链接，限制深度 12、音频文件 50,000；读取失败不会当作文件删除。移除文件夹或清空本地曲库只移除索引。下载先写临时文件，检查长度后发布，不覆盖已有文件。清理缓存会先停止当前播放，避免删除正在读取的缓存文件。

## 当前边界

系统托盘、全局快捷键和自动更新尚未接入；真实模式禁用托盘和 WebAPI 引擎选择。Slint 的玻璃模糊与 Electron CSS 仍有渲染差异。API 层已覆盖 Electron 音乐接口；首页横幅和云盘接口可由提供方调用，当前 Slint 页面未新增对应视图。歌手专辑/MV和账号歌单请求仍采用现有页面上限。

测试使用模拟账号/接口；真实账号的短信、二维码授权、会员音源及实际设备听感仍需账号与设备验收。

## 验证

```sh
npm run test:slint
cargo test --manifest-path native/Cargo.toml --no-default-features --locked
cargo check --manifest-path native/Cargo.toml --locked
```

新增测试覆盖完整 ID、业务错误、下载权限、会话持久化、歌词解析、本地扫描、乱序响应、超时不重试、子进程退出，以及注入纯 Rust 提供方后的页面、登录和双窗口偏好联通。native 的本地 HTTP/设备测试需能绑定回环端口并读取声卡。

歌单滚动的真实 OpenGL 窗口测量、封面更新的离线测量及验证边界见 [性能记录](tests/playlist-performance.md)。滚动回归还限制跨行时的模型读取次数，检查复用行的点击和数据更新。

主页保留原版的推荐布局，新增从 `album_newest` 获取的新碟上架和从 `toplist_detail` 获取的榜单速览，展示更新说明与前三首歌曲。新内容进入视口时独立加载，点击进入专辑或完整榜单。推荐歌单、推荐歌曲与新碟列表使用鼠标拖动横向浏览，也可使用左右按钮；滚轮在卡片上和空白处都用于上下滚动主页。卡片按视口循环复用，封面到达只更新对应项。交互与验证见 [主页记录](tests/home-display.md)。

全屏歌词的 Electron 布局对照、空时间戳过滤、混合译文/换行行高、平滑跟随与手动滚动恢复，以及 1600px 封面请求的验证见 [全屏显示记录](tests/fullscreen-display.md)。歌词按实际内容计算高度，相邻行间隔为 12px；高亮缩放不改变布局高度。翻译和发音沿用 Electron 的 350ms 内时间匹配规则。

歌词悬停沿用 Electron 的交互：只滚动时保留悬停行，实际移动鼠标后更新；中心指示选取距离视口中心最近的歌词，显示时间、虚线及播放按钮。指示线允许点击穿透，按钮上也能滚动，右键点击歌词区域恢复跟随；加载期间隐藏指示并禁用跳转。悬停显示相应的跳转时间提示。

真实窗口响应测试需连接 Wayland/X11 桌面，单独运行。默认用延迟 API 和本地大图测试网络等待、图片解码期间的导航点击、设置窗口和偏好同步；设置 `NCM_SLINT_TEST_LIVE_API=1` 使用真实 API。测试使用独立临时配置，检查封面确实加载、事件循环间隔小于 500ms，并输出截图目录。外部超时用于检测事件循环完全卡死的情况。

```sh
timeout 60s cargo test --manifest-path slint-ui/Cargo.toml --locked real_window_stays_responsive -- --ignored --nocapture
NCM_SLINT_TEST_LIVE_API=1 timeout 60s cargo test --manifest-path slint-ui/Cargo.toml --locked real_window_stays_responsive -- --ignored --nocapture
timeout 60s cargo test --manifest-path slint-ui/Cargo.toml --locked real_login_window_matches_electron_behavior -- --ignored --nocapture
```

登录回归对照 `tests/login-modal-ground-truth.json`，检查默认、错误和重新打开时的几何，以及独立表单、区号键盘输入、回车提交、加载期间防止重复提交、验证码倒计时、二维码过期刷新和关闭后的旧响应隔离。真实登录窗口测试使用延迟模拟 API，检查卡片/遮罩点击、两面的关闭按钮、扫码切回表单、保持登录标签和会话保存，并输出截图；不会发送真实短信或使用真实账号。更新 Electron 登录基准使用 `node_modules/.bin/electron --no-sandbox slint-ui/scripts/extract-electron-metrics.cjs --login-modal`（先构建 Electron）。

测试包含原有页面几何和动画回归，以及新建、编辑、删除、键盘操作、权限变化、空歌单隔离、重复提交和 Unicode 输入。窗口测试覆盖实际传给平台的尺寸约束、独立打开和关闭、重复打开、Esc、偏好双向同步及同一轮操作中的不同字段变更。设置窗口在 1000×800、1200×900 下对照 Electron DOM，新歌单管理控件在 1435×1000、620×720 下对照，容差为 0.5 个逻辑像素。620×720 仅用于无窗口系统的组件布局压力测试；实际主窗口受 1340×960 的最小尺寸限制。

用户气泡增加了逐帧悬浮回归：鼠标停在中心和上下边缘、跨菜单子项及离开时，底部头像只移动 5px，菜单不会反复展开/收起。展开后气泡、菜单和导航滚动区对照 `tests/sidebar-hover-ground-truth.json`。设置测试检查各标签页文字与控件的边界和间距，通用页标题、说明的纵向位置和行高也对照 Electron。

重新采集歌单基准（本地模拟 IPC，不使用真实账户）：

```sh
npx electron-vite build
node_modules/.bin/electron --no-sandbox slint-ui/scripts/extract-electron-metrics.cjs --playlist-management
```

输出为 `tests/playlist-management-ground-truth.json`，选择器缺失时会失败，不会把缺失控件当作通过。其余页面仍使用 `tests/electron-ground-truth.json` 中的历史基准，歌单控件在回归测试中使用新采集值覆盖。

采集设置窗口基准：在构建 Electron 后执行 `node_modules/.bin/electron --no-sandbox slint-ui/scripts/extract-electron-metrics.cjs --settings-window`，输出到 `tests/settings-window-ground-truth.json`。同时采集八个标签页内部布局，等待切换动画结束；用户气泡基准使用 `--sidebar-hover`。均使用本地模拟 IPC，支持以下截图环境变量。

可选截图：设置 `NCM_SLINT_SCREENSHOT_DIR=/tmp/ncm-slint-shots` 运行测试，会输出软件渲染的 PPM；设置 `NCM_ELECTRON_SCREENSHOT_DIR=/tmp/ncm-electron-shots` 运行采集脚本，会输出 PNG。Electron 截图像素尺寸取决于屏幕缩放，DOM 基准始终使用逻辑像素。

几何测试不代表整张界面截图已像素一致。Slint 当前的玻璃背景尚未复现 CSS backdrop-filter；字体栅格化、阴影、圆角裁剪和多行文本的行距也仍需在实际渲染后端继续对齐。
