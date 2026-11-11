# 可替换的音乐 API

Slint 只依赖 `src/runtime/api.rs` 中的 `MusicApi`：

```rust
pub trait MusicApi: Send + Sync {
    fn call(&self, operation: &str, params: serde_json::Value, cookie: &str) -> ApiFuture<'_>;
}
```

实现返回 `ApiResponse { status, body, cookie }`。`body` 保留服务端的数据字段；通用调用同时检查 HTTP 状态和业务 `code`。二维码轮询单独处理 800/801/802/803。歌曲、歌单、专辑、歌手 ID 在 Rust 内为完整字符串；Slint 的整数只用作内部句柄，不会把服务端 ID 转为 i32。

提供方只能在后台运行时创建和调用，UI 回调通过命令通道提交操作，禁止在 UI 线程创建 HTTP 请求、等待 Future 或执行图片解码。内置 Rust 提供方同样遵守这个边界。封面和二维码经后台 `images.rs` 解码成可跨线程传递的 `SharedPixelBuffer`，UI 只用 `Image::from_rgba8` 包装像素；不得在界面事件处理器中重新调用 `Image::load_from_path/load_from_data`。测试会验证 API 调用线程，并在真实窗口中注入慢请求和大图检查响应性。

## 提供方

默认的 `NativeApi` 直接使用 `dddqmmx/ncm-api-rs`，依赖固定到提交 `133b65bfe482e41ebccf018870d3fce07bf58eb3`，同时记录在 `Cargo.lock`。只启用 SDK，不启用它的 HTTP server feature；应用不启动 Node 子进程或本地 API 服务。构建、运行及 Rust 测试不需要安装 npm 依赖。旧的 `NCM_NODE`、`NCM_API_SIDECAR` 已移除。

一个 SDK 客户端复用 HTTP 连接；每次调用显式传入该请求的 cookie（匿名请求也显式传空值），不使用全局可变登录状态或 cookie jar。SDK 返回的登录 cookie 仍交由原有 `Services` 和 `Storage` 管理，旧会话文件无需迁移。接口适配位于 `src/runtime/api/native.rs`。

设置 `NCM_API_URL` 会替代内置 Rust 提供方，向该地址 POST v1 协议。远端地址需要 HTTPS，本机回环地址可使用 HTTP；不自动跟随重定向。此地址是**本应用的协议适配端点**，不能直接填写普通 NeteaseCloudMusicApi 或 ncm-api-rs HTTP 服务根地址；已有服务需要在前面转换操作名及响应格式。

## HTTP 协议 v1

仅自定义 HTTP 提供方使用这个信封；内置 Rust 提供方直接调用 SDK。

请求：

```json
{"version":1,"operation":"search","params":{"keywords":"音乐","type":1,"limit":100,"offset":0},"cookie":""}
```

响应：

```json
{"version":1,"status":200,"body":{"code":200,"result":{"songs":[],"songCount":0}},"cookie":[]}
```

HTTP 响应必须包含 `version: 1`，不要求请求 ID。`cookie` 是可选的 Set-Cookie 数组，登录也支持 `body.cookie`。密码和会话不进入命令行或应用日志。

内置提供方的参数与 cookie 合计限制 1 MiB，最多 64 个在途调用，单次调用 40 秒超时。取消或超时释放并发额度；创建、删除、收藏等操作不自动重发，超时后应刷新确认结果。HTTP 提供方同样使用 40 秒请求超时并限制响应为 16 MiB；内置 SDK 自行读取响应，当前没有流式响应大小上限。

## 操作覆盖

操作名与 Electron IPC 不同，适配层用以下稳定名称：

| 领域 | 操作 |
| --- | --- |
| 登录/会话 | `login`, `captcha_sent`, `login_qr_key`, `login_qr_create`, `login_qr_check`, `user_account`, `logout` |
| 发现/曲库 | `banner`, `user_cloud`, `search`, `song_detail`, `playlist_catlist`, `user_playlist`, `playlist_detail`, `recommend_resource`, `recommend_songs`, `personalized`, `personalized_newsong`, `album_newest`, `toplist_detail` |
| 播放/歌词/评论 | `song_url`, `lyric`, `like`, `likelist`, `comment_music` |
| 歌单写入 | `playlist_track_add`, `playlist_track_delete`, `playlist_create`, `playlist_update`, `playlist_delete` |
| 歌手/专辑/MV | `artist_detail`, `artist_top_song`, `artist_album`, `artist_mv`, `album`, `mv_url` |
| 下载 | `song_download_detail`, `song_download_url` |
| 诊断 | `configure_xeapi`, `ping` |

`src/runtime/api/native.rs` 的 `dispatch()` 是 Rust 适配的完整映射。下载音质检查、文件选择与写盘属于 Rust，不属于提供方。

首页发现接口 `album_newest` 与 `toplist_detail` 使用空参数、匿名请求；分别返回 `albums` 与 `list`。两个区域进入视口时独立请求，各自处理错误和重试，不阻塞每日推荐；卡片封面仍按可见范围请求。

需要保持的语义：

- `playlist_track_add/delete`：`params = { pid, tracks: [完整ID字符串] }`，将 ID 数组无损转换为逗号分隔字符串，使用 SDK `playlist_tracks` 的 `add/del`，不调用旧的 `playlist_track_add/delete` 模块。
- `playlist_update`：`params = { id, name, desc }`；分别 JSON 编码名称和简介，使用 batch，只改这两个字段，保留标签。两个子响应都必须成功；外层 200 或缺少子响应不代表完整保存。
- `song_url`：调用 SDK `song_url_v1`，直接使用 EAPI；不依赖 Node 的 XEAPI 公钥配置，不自动重试或降级音质。`configure_xeapi` 保留兼容操作名，内置提供方直接返回成功；`ping` 返回 `provider: "rust"`。
- 下载详情必须以当前会话重新请求；播放权限、歌曲最高音质、旧缓存不能替代当前账号的 `dl/dlLevel/dlLevels`。音源必须是完整文件，返回音质必须等于所选音质。
- `login_qr_key`：将 SDK 顶层 `unikey` 包装为应用使用的 `data.unikey`；`login_qr_create` 在后台阻塞任务中生成 PNG data URL，补齐 SDK 只返回 URL 的部分。`login_qr_check` 保留 800–803 状态及 Set-Cookie。
- SDK 成功响应保留原始 `body` 和 cookie；类型化业务错误转换为包含 `code`、`message` 的拒绝响应。当前 SDK 的部分错误只保留 `msg`，已经丢弃的原始响应字段无法恢复。网络/加密错误使用不含请求详情的提示，权限失败不重试。

## native 边界

`native` 的默认 feature `node` 保留 N-API 构建。Slint 使用 `native = { path = "../native", default-features = false }`，直接链接 `PlayerService`、`CacheService`，不需要 Node 来解码或输出音频。

Rust 控制器使用原有 native worker 的串行命令、完整播放快照、`wait_finished()` 结束原因以及边播边缓存实现。自然结束才自动切歌；停止、替换、错误不会被进度估算误判成播完。播放器命令、页面请求和登录会话分别检查版本，旧请求不能提交到新状态。

## 适配验证

`src/runtime/api/native/tests.rs` 通过回环 HTTP 服务调用真实 SDK，覆盖接口路径、EAPI 解密后的参数、会话隔离、歌单编辑、二维码图片及轮询、业务失败、超时和取消。`cargo test --locked runtime::api::` 不访问网易云。`live_native_api_smoke` 为显式运行的公开接口检查，不使用保存的账号。
