# Slint UI 结构检查

在 `slint-ui` 目录执行（只需要 Python 3.10+ 标准库）：

```sh
python3 scripts/check_ui_quality.py
python3 scripts/check_ui_quality.py --json
python3 -m unittest discover -s tests -p test_ui_quality.py -v
cargo test --locked
```

也可以在任何目录通过脚本的绝对路径调用；默认检查脚本所在项目。`--root PATH` 用于检查另一个包含 `ui/` 的项目。检测通过退出码为 0，存在问题为 1，命令参数错误为 2。普通输出为 `文件:行号:规则:原因`，JSON 输出包含每个文件、组件、全局对象的指标和所有问题，便于编辑器或 CI 使用。

## 检查规则

| 规则 | 限制与用途 |
| --- | --- |
| 文件体积 | 最多 600 个有效代码行，避免把多个页面或组件堆在同一文件 |
| 组件体积 | 单个组件最多 400 个有效代码行，避免只移动文件而不拆分组件 |
| 全局对象体积 | 每个 `global` 最多 **120 个有效代码行、80 个成员**（属性、回调、函数，包含私有成员）；压行不能绕过成员限制 |
| 嵌套 | 每个组件和全局对象最多 **5 层**，根节点计为第 1 层，布局、条件/循环元素、回调、函数和条件块都计入；第 6 层即失败，报告所属对象和最深块的实际行号 |
| 重复块 | 完整元素块达到 160 个相同 token 时报告；忽略空白和注释，保留标识符、字符串、数值及顺序，只报告最大的重复块 |
| 导入 | 检查本地 import/re-export 的文件存在性、项目边界和循环依赖；允许标准库 `std-widgets.slint` |
| 词法结构 | 未闭合字符串、块注释或大括号报错；无源文件也报错 |
| 布局常量 | 属性绑定、默认值和赋值中出现小数 `px` / `%` 字面量时报错，包括自定义长度属性；改用布局容器、内容测量或有明确意义的无单位比例 |
| 列表写入 | 禁止直接给任意对象的 `queue-songs/local-tracks/playlist-tracks/detail-tracks` 及其行字段赋值；包含独立业务状态与导入别名，通过控制层方法修改，避免界面绕过去重、选曲和保存逻辑 |

嵌套限制对整个 `ui/` 目录生效，不按缩进或换行计数：把代码压到一行也无法绕过。注释和字符串中的花括号不计入，兄弟元素不会累计深度；每个超限对象单独报告一次。JSON 中 `components` 和 `globals` 都提供 `nesting` 和 `nesting_line`，定位最深处；`globals` 另外提供成员数 `members`。

有效代码行按词法 token 所在行计算，空行、注释与字符串内的大括号不影响结构统计。阈值集中定义在程序的 `LIMITS`，没有忽略清单或历史违规基线。GitHub Actions 的 `Slint UI quality` 在相关源文件和检测程序变动时执行检查及其测试。

这是结构检查器，不替代 Slint 编译器、类型检查、交互测试和视觉验证。重复检测用于发现完整复制的控件，不声称能判断经过改名或改写的业务逻辑是否等价。

布局检查不会把 `font-size * 1.6`、缩放、透明度或贝塞尔曲线当成坐标，也不检查输入事件中用于判断鼠标是否移动的比较阈值。禁止把截图测量值移到另一个属性或简单取整来规避检查。控件可以保留设计规定的整像素尺寸、内边距和间距；文字行高、弹窗高度、按钮宽度和子组件位置应由实际内容及可用空间决定。

## 组件职责

- `ui/settings/settings_modal.slint`：窗口框架、导航和标签页装配。`*_tab.slint` 分别承载 8 个设置标签页，`controls.slint` 提供设置行、分组卡片、导航和文字布局，`aux_visibility_button.slint` 复用左右辅助按钮的可见性控件。
- `ui/components/comment_panel.slint`：评论列表滚动与配色装配；`comment_item.slint` 中的评论项、分区标题、作者/VIP 信息和引用回复组件只接收数据和配色，不直接读取页面状态。
- `ui/components/inset_shadow.slint`：通用内凹边缘，只接收圆角、阴影色和高光色；侧栏用户胶囊提供亮暗主题配色。九宫格保留圆角并随容器伸展，装饰层不处理输入；透明遮罩兼容软件渲染，避免依赖仅部分渲染器支持的内阴影、渐变描边或圆角裁剪。`assets/effects/` 中两张小型遮罩可通过 `scripts/generate_inset_shadow.py` 重建（生成时需要 Pillow，应用运行不需要）。
- `ui/components/player_fullscreen.slint`：全屏页面装配。歌词、评论、走带按钮和辅助按钮组各有独立模块。`lyric_lines.slint` 负责歌词行布局，`lyric_seek_indicator.slint` 负责滚动后的跳转指示器；播放进度条由 `fullscreen_progress.slint` 维护。`volume_popup.slint` 通过属性绑定音量和可用状态，供播放栏与全屏共用；`lyric_toolbar.slint` 复用发音和翻译切换控件。
- `ui/overlays/login_modal.slint`：登录弹窗生命周期与翻转动画。表单、二维码和通用登录控件分别维护，保留原来的焦点与动画行为。
- `ui/views/detail_views.slint`：详情头部和曲目列表布局。`detail_actions.slint` 接收数据与状态，负责详情页操作栏。
- `ui/components/media.slint`、`ui/overlays/modals.slint`：兼容导出入口，具体卡片、列表、行和弹窗位于独立模块，不把实现重新堆回入口。
- `ui/models.slint`：界面数据结构。`ui/state/*.slint` 按业务划分状态和操作，`ui/fixtures/*.slint` 提供只读演示数据；`state.slint` 只导出 Rust 绑定，不保存状态或转发操作。职责及依赖见 [状态模块说明](../ui/state/README.md)。

提取展示控件时优先使用属性和回调传递输入与操作；业务组件直接导入所需的业务状态，通用控件不直接绑定页面业务状态。组件变更应更新对应测试选择器；自适应布局使用多尺寸下的包含、不重叠、换行及滚动可达性断言，动画和交互断言继续保留。

歌曲列表与播放状态的同类问题审查及入口见 [歌曲列表操作审查](song-list-actions.md)。
