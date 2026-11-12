# Electron 客户端

NetEase Cloud Music 的 Electron + Vue + TypeScript 界面。仓库的首选客户端是 `slint-ui/`，这里作为二级客户端保留；两者共用 `native/` 的 Rust 播放、缓存与下载核心。

依赖、vite/builder 配置、脚本和构建输出都在本目录，`node_modules` 也只在这里。`native/index.node` 通过 `cd ../native && ../electron/node_modules/.bin/napi build` 编译到仓库根的 `native/`，打包时由 `electron-builder.yml` 的 `extraResources` 放进 `resources/native/index.node`。

## Install

```bash
cd electron
npm install
```

仓库根目录没有 `node_modules`，Slint 也不需要 npm 依赖；Electron 的依赖只装在本目录。

## Development

```bash
npm run dev        # 构建 native 并启动 electron-vite dev
npm run typecheck
npm run lint
npm run test:web
```

## Build

```bash
npm run build          # typecheck + native release + electron-vite build
npm run build:linux    # 打包 AppImage
npm run build:unpack   # 仅解包目录
npm run build:win
npm run build:mac
```

从仓库根目录可以用 `npm run build:electron`（等价于 `npm --prefix electron run build`）和 `npm run dev:electron`。
