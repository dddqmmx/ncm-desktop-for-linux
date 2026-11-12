# music

NetEase Cloud Music client for the Linux desktop. The repository holds one shared Rust audio/API
core (`native/`) and two front-ends on top of it.

| 目录 | 客户端 | 状态 |
| --- | --- | --- |
| `slint-ui/` | Slint + Rust 原生界面 | 首选，默认构建 |
| `electron/` | Electron + Vue + TypeScript | 二级，保留旧界面 |
| `native/` | 两者共享的播放、缓存与下载核心 | — |

根目录的 `npm run dev` / `build` / `test` 都指向 Slint；Electron 的脚本在 `electron/` 内部，通过
`npm run dev:electron` / `npm run build:electron` 从根目录调用。

## Slint（首选）

```bash
# 开发（保存 .slint 时热加载）
npm run dev

# release 构建：slint-ui/target/release/ncm-slint-ui
npm run build

# 测试与界面结构检查
npm run test
npm run check:slint
# 等价于
cargo run --manifest-path slint-ui/Cargo.toml --locked --features hot-reload
cargo build --manifest-path slint-ui/Cargo.toml --release --locked
cargo test --manifest-path slint-ui/Cargo.toml --locked
python3 slint-ui/scripts/check_ui_quality.py
```

构建与运行不需要 npm 依赖：Cargo 直接获取固定提交的 `dddqmmx/ncm-api-rs`，音频、缓存、下载和本地
扫描由 `native/` 进程内完成。渲染器、配置目录和测试细节见 [slint-ui/README.md](slint-ui/README.md)。

## Electron（二级）

Electron + Vue 客户端完整保留在 `electron/`，依赖、vite/builder 配置和输出都在该目录内。它和 Slint
共用 `native/` 的 Rust 源码与 `native/index.node` 构建产物。

```bash
cd electron
npm install            # Electron 的依赖只装在 electron/
npm run dev            # 构建 native 并启动 electron-vite dev
npm run build          # 类型检查 + native release + electron-vite build
npm run build:linux    # electron-builder 打包 AppImage
npm run build:unpack   # 仅解包目录
npm run lint
npm run test:web

cd ..
npm run dev:electron   # 等价于 npm --prefix electron run dev
npm run build:electron # 等价于 npm --prefix electron run build
```

`native/index.node` 由 `electron/` 中的 `build:native`/`build:native:dev` 通过
`electron/node_modules/.bin/napi` 编译到仓库根的 `native/`。打包时 electron-builder 用 `extraResources` 把
`../native/index.node` 放进 `resources/native/index.node`；开发时主进程按
`out/main` → 仓库根查找同一个文件。

## 其它脚本

```bash
npm run test:native-seek   # native 播放/拖动回归（cargo test + NAPI 播放探针）
```
