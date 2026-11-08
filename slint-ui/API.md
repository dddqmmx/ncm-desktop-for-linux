# 可替换的音乐 API

Slint 只依赖 `src/runtime/api.rs` 中的 `MusicApi`：

```rust
pub trait MusicApi: Send + Sync {
    fn call(&self, operation: &str, params: serde_json::Value, cookie: &str) -> ApiFuture<'_>;
}
```

实现返回 `ApiResponse { status, body, cookie }`。`body` 保留服务端的数据字段；通用调用同时检查 HTTP 状态和业务 `code`。二维码轮询单独处理 800/801/802/803。歌曲、歌单、专辑、歌手 ID 在 Rust 内为完整字符串；Slint 的整数只用作内部句柄，不会把服务端 ID 转为 i32。

提供方只能在后台运行时创建和调用，UI 回调通过命令通道提交操作，禁止在 UI 线程创建 HTTP 请求、等待 Future 或执行图片解码。纯 Rust 替换同样遵守这个边界。封面和二维码经后台 `images.rs` 解码成可跨线程传递的 `SharedPixelBuffer`，UI 只用 `Image::from_rgba8` 包装像素；不得在界面事件处理器中重新调用 `Image::load_from_path/load_from_data`。测试会验证 API 调用线程，并在真实窗口中注入慢请求和大图检查响应性。

## 提供方

默认的 `NodeApi` 启动 `sidecar/music-api.cjs`，只通过 stdin/stdout 交换 JSON Lines，不监听端口，不加载 Electron 或 native Node 插件。可执行文件从 `NCM_NODE` 读取，默认 `node`。脚本优先从 `NCM_API_SIDECAR`、可执行文件旁的 `sidecar/`、源码目录依次定位。部署时需同时带上 API 包及其 npm 依赖，或设置独立的 API 服务。

设置 `NCM_API_URL` 会完全跳过 Node 启动，向该地址 POST 同一份 v1 协议。远端地址需要 HTTPS，本机回环地址可使用 HTTP；不自动跟随重定向。此地址是**本应用的协议适配端点**，不能直接填写普通 NeteaseCloudMusicApi 服务根地址；已有服务需要在前面转换操作名及响应格式。

将来直接实现 Rust 网易云请求，只需实现 `MusicApi`，在 `configured_provider()` 中返回它；播放器、缓存、下载、会话存储和 UI 都可以保留。`backend.rs` 中的注入提供方测试已覆盖完全不启动 Node 的路径。

## 协议 v1

请求（`id` 由本地子进程传输分配；HTTP 不要求它）：

```json
{"version":1,"id":42,"operation":"search","params":{"keywords":"音乐","type":1,"limit":100,"offset":0},"cookie":""}
```

响应：

```json
{"version":1,"id":42,"status":200,"body":{"code":200,"result":{"songs":[],"songCount":0}},"cookie":[]}
```

子进程响应必须回传请求 `id`，支持乱序完成；HTTP 响应必须包含 `version: 1`。`cookie` 是可选的 Set-Cookie 数组，登录也支持 `body.cookie`。密码和会话只经进程管道/请求体传递，不进入命令行或应用日志。

请求上限 1 MiB，响应上限 16 MiB；本地最多 64 个在途请求，单次请求 40 秒超时。服务退出会使所有等待请求失败。超时后的修改可能已经被服务器执行，应用不会自动重发创建、删除、收藏等操作，用户应刷新确认。

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

`sidecar/music-api.cjs` 的 `METHODS` 与 `dispatch()` 是当前 Node 适配的完整映射。下载音质检查、文件选择与写盘属于 Rust，不属于提供方。

首页发现接口 `album_newest` 与 `toplist_detail` 使用空参数、匿名请求；分别返回 `albums` 与 `list`。两个区域进入视口时独立请求，各自处理错误和重试，不阻塞每日推荐；卡片封面仍按可见范围请求。

需要保持的语义：

- `playlist_track_add/delete`：`params = { pid, tracks: [完整ID字符串] }`，Node 转为 `playlist_tracks` 的 `add/del`，不调用旧的 `playlist_track_add/delete` 模块。
- `playlist_update`：`params = { id, name, desc }`；分别 JSON 编码名称和简介，使用 batch，只改这两个字段，保留标签。两个子响应都必须成功；外层 200 或缺少子响应不代表完整保存。
- `song_url`：优先 v1；仅遇到缺失 xeapi 公钥时刷新配置并重试一次，再尝试旧端点。普通权限/网络失败不会触发无限重试。旧音源根据实际码率补充 `level`。
- 下载详情必须以当前会话重新请求；播放权限、歌曲最高音质、旧缓存不能替代当前账号的 `dl/dlLevel/dlLevels`。音源必须是完整文件，返回音质必须等于所选音质。
- API 可能抛出普通 `{ status, body }` 对象，必须保留业务错误，不能一律当作空响应或成功。

## native 边界

`native` 的默认 feature `node` 保留 N-API 构建。Slint 使用 `native = { path = "../native", default-features = false }`，直接链接 `PlayerService`、`CacheService`，不需要 Node 来解码或输出音频。

Rust 控制器使用原有 native worker 的串行命令、完整播放快照、`wait_finished()` 结束原因以及边播边缓存实现。自然结束才自动切歌；停止、替换、错误不会被进度估算误判成播完。播放器命令、页面请求和登录会话分别检查版本，旧请求不能提交到新状态。
