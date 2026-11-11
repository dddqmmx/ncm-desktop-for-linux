# 设置窗口最小尺寸回归

2026-11-10，在 niri / Wayland 桌面复现：首次打开设置时最小尺寸为 1000×800，关闭并重新打开后，可以缩到测试请求的 100×100。125% 和 150% 缩放的显示器均出现该问题。

Slint 1.18.1 的 winit 后端在隐藏 Wayland 窗口时销毁原生窗口。再次显示时重建原生窗口，但保留的布局约束缓存认为最小尺寸没有变化，跳过向新窗口重新设置限制。只检查 Slint 声明或无桌面的测试后端，无法发现原生窗口已经丢失约束；只在 `show()` 返回时设置也不可靠，因为原生窗口的重建可能尚未完成。

`SettingsWindow` 统一声明最小宽高，Slint 布局和 Rust 原生窗口适配共用这两个值。`install_settings_size_constraints` 在新原生窗口首次绘制、显示缩放发生变化时同步最小尺寸，按当前 Slint 缩放转换为物理像素。每次重新打开都重新跟踪原生窗口，普通重绘不重复设置；事件继续交给 Slint 处理。

`src/window_size_tests.rs` 的真实桌面回归通过进程 ID 与窗口标题定位测试自己的设置窗口。它使用 niri IPC 请求缩到 100×100，然后检查实际逻辑尺寸为 1000×800，再请求放大到 1200×900；在所有启用的显示器上重复三轮关闭、重开，并覆盖通用和音频标签页。不会改变用户其他窗口或桌面配置。

```sh
cargo test --locked --offline real_settings_minimum_size_survives_reopen_and_monitor_changes -- --ignored --nocapture --test-threads=1
cargo test --locked --offline window_tests:: -- --test-threads=1
python3 scripts/check_ui_quality.py
```

本次验证：两个缩放比例、三轮重开共六次最小尺寸测量均为 1000×800，放大检查全部通过。五项窗口回归通过，包括拖动后仍可点击、下拉框键盘操作、窗口复用与偏好同步、BitPerfect 音量限制和 Slint 层约束。质量检测 86 个文件、0 个问题。
