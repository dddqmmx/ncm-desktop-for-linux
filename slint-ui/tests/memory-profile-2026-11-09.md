# 内存分析记录（2026-11-09）

结论：当前版本的主要问题是默认 Skia/Vulkan 渲染路径基础开销偏高，以及歌曲、封面索引缺少生命周期和容量控制。Node API 子进程、图片转换副本、字体和分配器保留的空闲页也占用内存。切到 release 能降低部分成本，但没有解决这些结构性问题。

本次只增加测量工具和测试探针，未改变生产程序行为，也未修改正在运行实例的设置、账户或缓存。

## 正在运行的实例

对启动本次调查时的 `target/debug/ncm-slint-ui` 及其 Node 子进程读取 `/proc/*/smaps_rollup`：

| 进程 | RSS / MiB | PSS / MiB | USS / MiB |
| --- | ---: | ---: | ---: |
| Slint 主进程 | 579.7 | 503.8 | 482.0 |
| Node API 子进程 | 95.7 | 52.9 | 42.2 |
| 合计 | 675.4 | 556.7 | 524.1 |

NVIDIA 对该 Slint 实例另外报告 379 MiB GPU 内存。它是独立口径，不能与 RSS/PSS 直接相加。`ps` 曾显示约 720 MiB 的 RSS 合计；本报告统一用 smaps 的同一口径，避免混用内核计数。

主进程 PSS 按映射大致拆分：匿名映射、heap、stack 296.8 MiB；可执行文件映射 84.4 MiB；NVIDIA 设备映射 44.1 MiB；NVIDIA 库 26.3 MiB；字体 47.8 MiB；其他约 4.3 MiB。匿名内存包含应用、分配器和渲染驱动的分配，不能直接全部归为歌曲或封面。映射分类与 rollup 采用分步读取，且存在 KiB 舍入。

这份运行中快照未读取堆对象，无法推断该实例当前缓存了多少歌曲或图片；以下受控实验用于验证成因，不把模拟数据量当成用户真实浏览历史。

## 隔离空首页对照

环境：Linux 7.2.14-arch1-1、niri/Wayland、RTX 3060 Laptop、NVIDIA 615.78.08、Slint 1.18.1、Node v26.13.0。代码基线 `f80b59c`，Electron 使用当前工作区经 `electron-vite build` 生成的页面。

窗口请求 1435×1000，但 niri 实际分配 **1340×1001 逻辑像素**，Slint 稳定后报告 **2010×1502 物理像素、scale=1.5**。应以实际尺寸解释测量结果。

所有实例顺序运行；独立配置目录与缓存；无账户、无播放、无封面；本地模拟 API。Slint 使用正式可执行文件，包含 Rust 后台和 native 初始化，HTTP mock 替代 Node API；Electron 使用真实页面和 preload，但主进程业务、API 与 native IPC 由夹具替代，开启硬件加速并汇总整个进程树。因此这组数据衡量空首页基础成本，并非两个完整客户端在真实账户下的同负载基准。

| 空首页 | RSS / MiB | PSS / MiB |
| --- | ---: | ---: |
| Slint debug，Skia/Vulkan | 390 | 246 |
| Slint debug，FemtoVG/OpenGL | 276 | 169 |
| Slint debug，软件渲染 | 138 | 71 |
| Slint release，Skia/Vulkan | 336–337 | 226–258 |
| Slint release，FemtoVG/OpenGL | 171–172 | 103–106 |
| Slint release，软件渲染 | 92–93 | 58–59 |
| Electron 页面夹具，整个进程树 | 722 | 315 |

数值为本机采样的约数，每个实例取两个稳定样本，原始数值见同目录 JSON。第一轮测试时原来的 debug 实例仍运行，新 debug 实例会与其共享可执行文件页，因此 PSS 的 debug/release 差值不能单独代表优化构建的全部收益。最后复测时原实例已经退出，Vulkan 的 PSS 从 226 增至 258 MiB，主要变化来自字体和 NVIDIA 库的共享页分摊：Pss_File 从约 94 增至 126 MiB，Pss_Anon 仍约 132 MiB，RSS 仍约 336 MiB。这并非同一进程逐渐增长。报告保留区间和相关分类，避免把系统共享状态变化误当作泄漏。

Electron 夹具报告 `gpu_compositing=enabled`、`opengl=enabled_on`，但 Wayland 日志提示 Vulkan 不兼容，不能把它称为与 Slint 相同的 Vulkan 渲染路径。NVIDIA 的 compute-apps 查询也可能漏掉纯图形客户端，JSON 中无记录不代表显存为零。

发布版默认 Vulkan 相比软件路径多约 168–199 MiB PSS，相比 OpenGL 多约 123–152 MiB。渲染器选择对本机内存的影响比简单切换 release 更大。软件模式的帧率、视觉兼容性不在本轮测试范围内，不能仅凭内存数字决定默认渲染器。

## 已复现的无界数据保留

[View](../src/runtime/view.rs) 的 `register()` 持续向 `songs` 写入，`register_cover()` 持续向 `cover_refs` / `cover_handles` 写入；[Ids](../src/runtime/data.rs) 的身份索引也持续扩展。导航清空 `displayed` 和界面模型时，这些容器仍保留旧实体。封面像素的 64 MiB 预算不限制这些元数据。

`profile_memory_navigation_retention` 使用真实 `View.apply` 和导航逻辑，加载后立即清空页面模型，没有播放队列、没有图片解码。每轮在 glibc 下显式 `malloc_trim(0)`，尽量排除空闲页保留的干扰：

| 场景 | 最终保留歌曲 | 最终封面 URL 索引 | PSS / MiB |
| --- | ---: | ---: | ---: |
| 初始 | 0 | 0 | 62 |
| 同一批 1 万首反复加载 10 次 | 10,000 | 10,000 | 84 |
| 10 批不同歌曲，每批 1 万首 | 100,000 | 100,000 | 200 |

最后一轮 `displayed=0`、`queue=0`，仍保留所有 10 万首歌曲。这证明浏览更多不同内容会扩大仍有引用的缓存。压力数据规模用于展示增长趋势；未证明当前用户实例的全部额外内存都来自这个问题。

修复应给歌曲与身份/封面索引统一规定存活范围：当前页面、待播队列、当前歌曲、本地曲库、仍展示的首页/侧栏实体；其他实体淘汰或进入有容量上限的缓存。必须保持现有句柄有效性，不能简单 `songs.clear()` 破坏播放和菜单操作。

## 封面、分配器和登录动画

[CoverCache](../src/runtime/covers.rs) 限制解码像素为 64 MiB、1024 项，并允许固定保留的图片。受控测试投递 1500 张 128×128 图片后，缓存剩 1024 张、恰好 64 MiB，模型中的淘汰引用也被清理。

| debug 阶段 / PSS MiB | Vulkan | OpenGL | 软件 |
| --- | ---: | ---: | ---: |
| 空首页 | 263.1 | 192.2 | 96.9 |
| 1 万首，无封面 | 291.0 | 217.0 | 121.8 |
| 500 张封面，31.25 MiB 像素 | 322.6 | 247.6 | 150.9 |
| 1500 张投递，缓存限制至 64 MiB | 356.6 | 281.6 | 185.2 |
| 显式释放全部模型和 View，空闲后 | 306.7 | 230.2 | 133.8 |
| 诊断性 malloc_trim 后 | 273.3 | 195.8 | 100.3 |

这里的手动测试可执行文件与正式程序不同，不应把其绝对值直接和上一表混用。阶段差值说明：释放对象后，约 33–34 MiB 空闲内存仍被分配器保留。release/Vulkan 阶段实验也复现了保留，trim 前约 292 MiB，trim 后约 240 MiB。`malloc_trim` 只用于诊断，本次没有把它加入应用定时器。

64 MiB 也不是整个图片链路的总预算：Slint 1.18.1 的 `i-slint-renderer-skia/cached_image.rs` 在 RGBA 转换时调用 `skia_safe::Data::new_copy`，有额外 CPU 像素副本，GPU 纹理另计。[解码器](../src/runtime/images.rs) 的缩略图转换和 `SharedPixelBuffer::clone_from_slice` 还产生短暂副本；队列虽有界，8 张 1600×1600 RGBA 输出仍约 78.1 MiB。这里未测出用户实例中这些副本各自的实际字节数。

[登录翻转](../src/login_animation.rs) 同时保存正反完整帧和条带模型，关闭弹窗时 [AppState](../ui/state.slint) 未清空它们。手动捕获前后约增加 19–22 MiB PSS，隐藏后仍保留相近用量。建议动画结束或关闭后释放条带和完整帧，并确认完整帧是否仍有生产用途。这个增量包含捕获渲染器及分配器开销，不能全部当成可立即回收的像素。

预先构建设置窗口及登录捕获窗口只增加数 MiB；设置窗口显示/隐藏在该实验中也只有约 1 MiB 增量。现有证据不支持把隐藏设置窗口视为主要原因。

## API 和音频

测量时的 `sidecar/music-api.cjs` 启动独立 Node，并整体加载 `@neteasecloudmusicapienhanced/api`。隔离 require 实验加载了 844 个模块，Node RSS 由约 49 MiB 增至 95 MiB；运行中 sidecar PSS 约 53 MiB。2026-11-10 已将 Slint 默认提供方替换为进程内 `ncm-api-rs`，这组 sidecar 内存数据仅代表替换前状态；替换后的整体内存收益需要重新测量。

原生 [audio/player.rs](../../native/src/audio/player.rs) 的输出 ring buffer 长度为 6 秒；以 48 kHz、双声道、4 字节样本计算约 2.2 MiB。高采样率、多声道和解码器另有成本。本轮无播放的页面实验已经复现高基线和数据增长，因此优先处理渲染与数据生命周期；未测量当前正在运行实例的音频堆。

## 建议处理顺序

1. 修复歌曲、封面引用与身份索引的无界保留；以不同歌单反复导航后的稳定占用作为验收条件，并验证播放、菜单、首页和队列句柄。
2. 在同一设备上综合内存、滚动帧率和画面一致性评估 release/OpenGL；当前默认 Vulkan 的额外基础成本最大。
3. 释放登录动画临时帧，进一步给可见图片、渲染器副本与解码工作集制定总预算。
4. 再评估分配器保留策略、Node API 按需加载、字体集合与构建特性。debug 文件在磁盘上约 722 MiB、release 约 69 MiB，不能把这些文件大小直接当作常驻内存或预期节省量。

## 复现与验证

从 `slint-ui` 运行；图形测试需真实桌面，分别启动，避免相互影响：

```sh
# 只读采样正在运行的主 PID，自动包含子进程
python3 scripts/profile-memory.py <PID> --samples 3 --output /tmp/ncm-live.json

# 正式 release 程序：隔离配置、空首页、本地模拟 API，依次测三个渲染器
cargo build --release --locked
python3 scripts/profile-memory-fixture.py target/release/ncm-slint-ui --output /tmp/ncm-release.json

# 从仓库根目录先运行 node_modules/.bin/electron-vite build，再回到 slint-ui
python3 scripts/profile-memory-fixture.py ../node_modules/electron/dist/electron --electron --output /tmp/ncm-electron.json

# 分阶段窗口/封面/登录测量，替换 renderer 为 opengl 或 software
NCM_SLINT_TEST_RENDERER=vulkan NCM_MEMORY_TRIM=1 cargo test --locked profile_memory_stages -- --ignored --nocapture --test-threads=1

# 元数据保留：不同批次与重复相同批次两个独立进程
NCM_MEMORY_TRIM=1 cargo test --locked profile_memory_navigation_retention -- --ignored --nocapture --test-threads=1
NCM_MEMORY_TRIM=1 NCM_MEMORY_REPEAT_SAME=1 cargo test --locked profile_memory_navigation_retention -- --ignored --nocapture --test-threads=1
```

`NCM_MEMORY_TRIM` 仅在 Linux/glibc 手动探针中调用，生产可执行文件没有该路径。所有夹具使用模拟数据，不读取账户。图形窗口会短暂出现，然后自动退出。

验证：`cargo test --locked` 为 49 passed / 13 ignored；三个 debug 渲染器的窗口阶段实验通过，release/Vulkan 阶段实验通过；两个元数据保留对照通过；release 构建、Electron 构建与独立夹具完成。测量结果摘录保存在 [JSON](memory-profile-2026-11-09.json)。
