# Vulkan 渲染验证

2026-11-09，Slint 1.18.1，真实 Wayland 桌面，NVIDIA GeForce RTX 3060 Laptop GPU，驱动 615.78.08，设备报告 Vulkan 1.4.351。构建为 Cargo debug。

默认构建已启用 `vulkan` feature（`slint/renderer-skia-vulkan`），默认配置使用 Vulkan。`src/rendering.rs` 按保存的 `renderer` 配置选择 Skia Vulkan、FemtoVG OpenGL 或软件渲染；主程序在创建窗口之前读取配置。真实窗口回归共用选择逻辑，但从独立测试配置选择模式，不读取用户的配置。

首页真实窗口测试的 Vulkan Loader 日志确认实际创建设备：

```text
Using "NVIDIA GeForce RTX 3060 Laptop GPU" with driver: "/usr/lib/libGLX_nvidia.so.615.78.08"
```

这里的 `libGLX_nvidia` 是 NVIDIA Vulkan ICD 加载的驱动库名称；设备创建来自 Vulkan Loader，不能按库名称判定为 OpenGL。

## 显示与交互

- 常规回归：43 项通过，7 项需要手动执行；包含渲染配置迁移、保存恢复和关闭前保存。
- 首页真实窗口：浅色、深色、页面纵向滚动、卡片横向滚动、最小窗口与截图通过，封面使用此前缓存的公开 API 数据。
- 全屏真实窗口：1600px 封面、歌词跟随、悬停、中心指示、相邻行间距与手动滚动恢复通过。跟随过程中/结束后的歌词中心为 491.44 / 418.67 逻辑像素。
- 登录真实窗口：使用模拟账号，密码/验证码/二维码页面及离屏截图翻转通过，最大事件循环间隔约 205 ms。
- 主窗口与设置窗口：后台加载 5 张封面并验证交互和偏好同步，最大事件循环间隔约 61 ms。

全屏测试会在截图后的下一轮事件循环启动歌词动画，使 Slint 的动画时钟先更新，避免 GPU 读回耗时被算进动画。测试注入自己的指针事件，并过滤桌面真实鼠标事件，避免测试中的悬停位置被外部移动覆盖。这些调整只用于测试，没有改变产品中的歌词或鼠标交互。

## 滚动测量

沿用 `profile_real_playlist_scrolling`：一万首模拟歌曲、前 500 首带 128px 封面、1435×1000 物理像素、缩放 1、预热 2 秒、120 次滚轮事件。测量时不并行编译或运行其他窗口测试，不使用真实账号。

| 指标 | FemtoVG OpenGL 中位数 / P95 | Skia Vulkan 中位数 / P95 |
| --- | --- | --- |
| 滚轮事件处理 | 4.16 / 5.04 ms | 4.09 / 5.11 ms |
| 事件循环定时器间隔 | 16.77 / 22.95 ms | 17.20 / 20.48 ms |

这次对照中的滚轮处理耗时接近，没有测出明显提速。单次 debug 测量、两种不同的渲染器和各自的重绘机制，不足以推断 Vulkan 相比 OpenGL 的普遍性能。OpenGL 对照的渲染通知区间为 11.13 / 13.22 ms，但 Vulkan 没有对应指标，不能将它们作渲染耗时比较。

Skia Vulkan 的稳定 API 不提供本测试使用的 `BeforeRendering` / `AfterRendering` 通知。因此 Vulkan 测试不安装这类通知（安装还会影响 Skia 的局部重绘），只报告滚轮处理和事件循环间隔；缺少渲染样本会显示 `unavailable`，不以 0 ms 代替。事件循环间隔不等于 GPU 帧时间。

## 复测

以下命令在 `slint-ui` 目录运行。真实窗口测试必须能访问 Wayland/X11，且每个命令单独执行。

```sh
cargo test --locked --features vulkan -- --test-threads=1
VK_LOADER_DEBUG=driver cargo test --locked --features vulkan real_home_scroll_and_theme_layout -- --ignored --nocapture --test-threads=1
NCM_SLINT_SCREENSHOT_DIR=/tmp/ncm-vulkan-fullscreen cargo test --locked --features vulkan real_fullscreen_artwork_and_lyric_follow -- --ignored --nocapture --test-threads=1
cargo test --locked --features vulkan real_login_window_matches_electron_behavior -- --ignored --nocapture --test-threads=1
cargo test --locked --features vulkan real_window_stays_responsive -- --ignored --nocapture --test-threads=1
cargo test --locked --features vulkan profile_real_playlist_scrolling -- --ignored --nocapture --test-threads=1
# 同一测试的 OpenGL 对照（同一个二进制）
NCM_SLINT_TEST_RENDERER=opengl cargo test --locked profile_real_playlist_scrolling -- --ignored --nocapture --test-threads=1
```

设置中的下拉菜单提供 Vulkan（默认）、OpenGL 和软件渲染（关闭 GPU 加速），更改后提示重启生效。旧配置缺少 `renderer`、未知的渲染模式均回到 Vulkan；保存渲染选择保留其他音频偏好。回归还覆盖窗口之间同步、重启读取和修改后立即退出时的保存。

三种模式的真实窗口回归分别通过：读取保存的配置后，主窗口、设置窗口、渲染下拉菜单和截图均正常。软件模式额外设置 `SLINT_BACKEND=winit-skia` 和无效的 `VK_DRIVER_FILES`，仍按磁盘中的 `software` 配置正常启动，且软件窗口不提供 GPU 渲染通知。Vulkan 模式的 Loader 日志再次确认使用 RTX 3060。

逐个验证三种模式读取磁盘配置后打开主窗口、设置窗口和渲染下拉菜单：

```sh
cargo test --locked real_saved_renderer_opens_main_and_settings -- --ignored --nocapture --test-threads=1
NCM_SLINT_TEST_RENDERER=opengl cargo test --locked real_saved_renderer_opens_main_and_settings -- --ignored --nocapture --test-threads=1
NCM_SLINT_TEST_RENDERER=software cargo test --locked real_saved_renderer_opens_main_and_settings -- --ignored --nocapture --test-threads=1
```
