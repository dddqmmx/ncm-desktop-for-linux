# ncm-desktop-for-linux

Linux 桌面上的第三方网易云音乐客户端。仓库里有一份共享的 Rust 播放/缓存/下载核心（`native/`），
上面接了两个前端：

| 目录 | 客户端 | 状态 |
| --- | --- | --- |
| [`slint-ui/`](slint-ui/) | Slint + Rust 原生界面 | **首选**，用 Cargo 构建 |
| [`electron/`](electron/) | Electron + Vue + TypeScript | 二级，保留旧界面 |
| [`native/`](native/) | 两者共用的音频、缓存、下载核心 | — |
| [`ncm-api-rs/`](https://github.com/dddqmmx/ncm-api-rs) | Rust API SDK | 独立 Git 仓库 |

Slint 客户端完全用 Cargo 构建和运行，不需要 npm；Electron 的 npm 脚本都在 `electron/` 内部。

## Slint 客户端（首选）

Rust 进程内完成 API 请求、解码、缓存与输出，构建和运行都不需要 Node.js 或 npm 依赖。

### 依赖

- Rust（`slint-ui` 用 edition 2021，`native` 用 edition 2024）
- Linux 上的 ALSA（`libasound2-dev`）、`pkg-config`、`cmake`（编译 bundled libopus）
- Vulkan 驱动或 OpenGL 驱动；没有 GPU 时可在设置里切到软件渲染

```bash
# Debian/Ubuntu
sudo apt install pkg-config libasound2-dev cmake libvulkan-dev \
  libxkbcommon-dev libwayland-dev libgl1-mesa-dev
```

### 构建与运行

```bash
# 首次准备 API SDK；已克隆时直接使用现有目录中的源码
git clone git@github.com:dddqmmx/ncm-api-rs.git ncm-api-rs
# 根 Cargo.toml 是包含 slint-ui 和 native 的工作区，API SDK 使用本地 path 依赖
cargo run --locked --features hot-reload     # 开发，保存 ui/ 下的 .slint 后界面就地刷新
cargo build --locked --release               # release 产物：target/release/ncm-slint-ui
cargo test --workspace --locked              # 界面与 native 核心的测试
cargo test --manifest-path ncm-api-rs/Cargo.toml --locked --lib --test error --test query # API SDK 离线回归

python3 slint-ui/scripts/check_ui_quality.py # 界面结构/体积/嵌套/导入依赖检查
```

根目录默认只构建 Slint 客户端及其 Rust 依赖，避免同时构建 Electron 的 NAPI 模块。
构建 Electron 的 native 模块使用 `cargo build -p native` 或现有 npm 脚本；测试整个工作区用 `--workspace`。

Slint 生成的界面绑定位于独立的 `slint-ui/ui-bindings` crate，普通构建与测试共享编译产物。
只修改 Rust 业务逻辑时可复用界面编译缓存；修改 `.slint` 或界面资源时自动重新生成。
日常检查可用 `cargo check --locked` 跳过链接；界面布局迭代使用上面的 `hot-reload` 命令。

`--demo` 用离线演示数据运行，不启动 API、不读真实会话：

```bash
cargo run --locked --features hot-reload -- --demo
```

### 功能

- 登录：手机密码 / 验证码、二维码登录（轮询与过期刷新）、保持登录、退出
- 内容：推荐、分页搜索、喜欢的音乐、歌单、歌手、专辑、MV、歌词/译文、评论
- 歌单：云端创建 / 编辑 / 删除 / 增删曲目，失败保留草稿
- 播放：队列与「添加到下一曲」、随机 / 单曲循环、进度拖动、歌词跳转、真实输出设备、
  软件音量、独占 / BitPerfect
- 本地与缓存：本地文件夹扫描、缓存容量与预下载上限、清理；下载带音质选择、原生保存窗口和完整性检查
- 界面：隐藏系统标题栏的自绘窗口、独立设置窗口、Vulkan / OpenGL / 软件三种渲染器

界面细节、渲染器说明、性能与回归测试的记录都在 [slint-ui/README.md](slint-ui/README.md)。

### 数据目录

| 变量 | 用途 |
| --- | --- |
| `NCM_DATA_DIR` | 覆盖偏好/会话/本地曲库目录（默认 `~/.config/ncm-slint`） |
| `NCM_CACHE_DIR` | 覆盖缓存目录（默认 `~/.cache/ncm-slint`） |
| `NCM_API_URL` | 用 v1 HTTP 提供方替代内置 Rust API |

Slint 的数据目录与 Electron 相互独立，不自动导入旧登录信息。会话文件只靠文件权限保护，未接系统密钥环，
密码不落盘。

播放与 API 诊断日志位于 `~/.config/ncm-slint/diagnostics.jsonl`（遵循 `NCM_DATA_DIR`），
记录失败阶段、状态码、音质、设备和原因，5 MiB 自动轮换；诊断详情见 [API.md](slint-ui/API.md)。

## Electron 客户端（二级）

Electron + Vue 界面完整保留在 [`electron/`](electron/)，依赖、vite/electron-builder 配置、`node_modules`
和构建输出都在该目录内；它与 Slint 共用 `native/` 的 Rust 源码和 `native/index.node`。

```bash
cd electron
npm install            # Electron 的依赖只装在这里
npm run dev            # 构建 native 并启动 electron-vite dev
npm run build          # typecheck + native release + electron-vite build
npm run build:linux    # electron-builder 打包 AppImage
npm run lint
npm run test:web
```

细节见 [electron/README.md](electron/README.md)。

## native 回归

```bash
node scripts/native-seek-regression.cjs   # native 播放/拖动回归：cargo test + 真实 NAPI 播放探针
```

## 当前边界

系统托盘、全局快捷键和自动更新尚未接入。Slint 的玻璃模糊与 Electron 的 CSS `backdrop-filter` 仍有渲染差异；
字体栅格化、阴影、圆角裁剪和多行行距也在继续对齐。测试使用模拟账号与接口，真实账号的短信、二维码授权、
会员音源和实际设备听感仍需自己验收。

## 许可证

[MIT](LICENSE)
