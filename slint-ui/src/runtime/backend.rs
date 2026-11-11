use super::{
    api::{self, MusicApi},
    data::{self, items, text, Preferences, Song, Storage},
    images::{DecodedImage, ImageLoader},
    library,
    playback::{clamp_position, PlaybackState},
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};
use tokio::sync::mpsc;
#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    pub data: Value,
    pub token: u64,
}
#[derive(Debug)]
pub struct Event {
    pub name: String,
    pub data: Value,
    pub token: u64,
    pub epoch: u64,
    pub error: Option<String>,
}
#[derive(Default)]
struct Session {
    cookie: String,
    uid: String,
}
pub struct Services {
    api: Result<Arc<dyn MusicApi>, String>,
    storage: Storage,
    cache: Arc<native::CacheService>,
    image_loader: ImageLoader,
    player: Option<Arc<native::PlayerService>>,
    prefs: RwLock<Preferences>,
    applied_prefs: Mutex<Preferences>,
    session: Mutex<Session>,
    epoch: AtomicU64,
    auth_task: Mutex<Option<tokio::task::AbortHandle>>,
    play_version: AtomicU64,
    active_play_token: AtomicU64,
    playback_gate: tokio::sync::Mutex<()>,
    mutation_gate: tokio::sync::Mutex<()>,
    library_gate: tokio::sync::Mutex<()>,
    settings_gate: tokio::sync::Mutex<()>,
    device_token: AtomicU64,
    metadata: Mutex<Option<String>>,
    events: std::sync::mpsc::Sender<Event>,
}
impl Services {
    fn event(&self, name: &str, token: u64, epoch: u64, result: Result<Value, String>) {
        if epoch != self.epoch.load(Ordering::Acquire) {
            return;
        }
        let (data, error) = match result {
            Ok(v) => (v, None),
            Err(e) => (Value::Null, Some(e)),
        };
        let _ = self.events.send(Event {
            name: name.into(),
            data,
            token,
            epoch,
            error,
        });
    }
    // Cache public metadata by account; URLs, mutations and permissions always go to the provider.
    async fn call(&self, op: &str, p: Value, cookie: &str) -> Result<Value, String> {
        use sha2::{Digest, Sha256};
        let bucket = match op {
            "lyric" => Some("lyric"),
            "album" | "artist_detail" | "artist_top_song" | "artist_album" | "artist_mv" => {
                Some("entity")
            }
            _ => None,
        };
        let hash = |bytes: &[u8]| {
            Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let key = format!(
            "{op}:{}:{}",
            hash(cookie.as_bytes()),
            hash(p.to_string().as_bytes())
        );
        if let Some(bucket) = bucket {
            if let Ok(Some(cached)) = self.cache.get_json(bucket.into(), key.clone()).await {
                if let Ok(body) = serde_json::from_str(&cached) {
                    return Ok(body);
                }
            }
        }
        let body = self
            .api
            .as_ref()
            .map_err(Clone::clone)?
            .call(op, p, cookie)
            .await?
            .checked()?
            .body;
        if let Some(bucket) = bucket {
            let _ = self
                .cache
                .put_json(bucket.into(), key, body.to_string())
                .await;
        }
        Ok(body)
    }
    fn cookie(&self) -> String {
        self.session.lock().unwrap().cookie.clone()
    }
    fn player(&self) -> Result<Arc<native::PlayerService>, String> {
        self.player
            .clone()
            .ok_or_else(|| "音频设备初始化失败，请检查系统输出设备后重启".into())
    }
    async fn page(
        &self,
        data: &Value,
        cookie: &str,
        token: u64,
        epoch: u64,
    ) -> Result<Value, String> {
        let kind = text(&data["kind"]);
        let id = text(&data["id"]);
        let result = match kind.as_str() {
            "search" => {
                self.call(
                    "search",
                    json!(
                        { "keywords" : data["query"], "type" : 1, "limit" : 100,
                        "offset" : data["offset"].as_u64().unwrap_or(0) }
                    ),
                    cookie,
                )
                .await?
            }
            "playlist" => {
                let mut body = self
                    .call("playlist_detail", json!({ "id" : id }), cookie)
                    .await?;
                // Publish the header before fetching the complete track list.
                let mut metadata = body["playlist"].clone();
                if let Some(metadata) = metadata.as_object_mut() {
                    metadata.remove("tracks");
                    metadata.remove("trackIds");
                }
                self.event(
                    "playlist-metadata",
                    token,
                    epoch,
                    Ok(json!({ "id": id, "playlist": metadata })),
                );
                let ids: Vec<_> = items(&body["playlist"]["trackIds"])
                    .iter()
                    .map(|v| text(&v["id"]))
                    .collect();
                if ids.len() > items(&body["playlist"]["tracks"]).len() {
                    let mut tracks = vec![];
                    for chunk in ids.chunks(300) {
                        let b = self
                            .call("song_detail", json!({ "ids" : chunk.join(",") }), cookie)
                            .await?;
                        tracks.extend_from_slice(items(&b["songs"]));
                    }
                    body["playlist"]["tracks"] = json!(tracks);
                }
                body
            }
            "album" => self.call("album", json!({ "id" : id }), cookie).await?,
            "artist" => {
                let (detail, top, album, mv) = tokio::join!(
                    self.call("artist_detail", json!({ "id" : id }), cookie),
                    self.call("artist_top_song", json!({ "id" : id }), cookie),
                    self.call("artist_album", json!({ "id" : id, "limit" : 100 }), cookie),
                    self.call("artist_mv", json!({ "id" : id, "limit" : 100 }), cookie)
                );
                json!(
                    { "detail" : detail ?, "top" : top ?, "album" : album ?, "mv" : mv ?
                    }
                )
            }
            "favorites" => {
                let uid = self.session.lock().unwrap().uid.clone();
                if uid.is_empty() {
                    return Err("请先登录以同步喜欢的音乐".into());
                }
                let body = self
                    .call("likelist", json!({ "uid" : uid }), cookie)
                    .await?;
                let ids: Vec<_> = items(&body["ids"]).iter().map(text).collect();
                let mut songs = vec![];
                for chunk in ids.chunks(300) {
                    let b = self
                        .call("song_detail", json!({ "ids" : chunk.join(",") }), cookie)
                        .await?;
                    songs.extend_from_slice(items(&b["songs"]));
                }
                json!({ "songs" : songs, "ids" : ids })
            }
            "cloud" => {
                self.call(
                    "user_cloud",
                    json!({ "limit" : 1000, "offset" : 0 }),
                    cookie,
                )
                .await?
            }
            _ => return Err("未知页面".into()),
        };
        let images = json!({});
        Ok(json!(
            { "kind" : kind, "id" : id, "body" : result, "images" : images,
            "autoplay" : data["autoplay"], "offset" : data["offset"] }
        ))
    }
    async fn home(&self, cookie: &str) -> Result<Value, String> {
        let (playlists, songs, categories, banners) = tokio::join!(
            self.call(
                if cookie.is_empty() {
                    "personalized"
                } else {
                    "recommend_resource"
                },
                json!({ "limit" : 30 }),
                cookie
            ),
            self.call(
                if cookie.is_empty() {
                    "personalized_newsong"
                } else {
                    "recommend_songs"
                },
                json!({ "limit" : 10 }),
                cookie
            ),
            self.call("playlist_catlist", json!({}), cookie),
            self.call("banner", json!({ "type" : 0 }), cookie)
        );
        let body = json!(
            { "playlists" : playlists ?, "songs" : songs ?, "categories" : categories.unwrap_or_else(|_|json!({"sub":[]})),
            "banners" : banners.unwrap_or_else(|_|json!({"banners":[]})) }
        );
        let images = json!({});
        Ok(json!({ "body" : body, "images" : images }))
    }
    async fn account(&self, cookie: &str, epoch: u64) -> Result<Value, String> {
        let body = self.call("user_account", json!({}), cookie).await?;
        let uid = text(&body["profile"]["userId"]);
        if uid.is_empty() {
            return Err("登录状态已失效，请重新登录".into());
        }
        if epoch != self.epoch.load(Ordering::Acquire) {
            return Err("登录请求已取消".into());
        }
        let (playlists, likes) = tokio::join!(
            self.call(
                "user_playlist",
                json!({ "uid" : uid, "limit" : 1000 }),
                cookie
            ),
            self.call("likelist", json!({ "uid" : uid }), cookie)
        );
        let body = json!(
            { "profile" : body["profile"], "playlists" : playlists ? ["playlist"],
            "likes" : likes ? ["ids"] }
        );
        let images = json!({});
        Ok(json!({ "body" : body, "images" : images }))
    }
    async fn authenticate(
        &self,
        cookie: String,
        remember: bool,
        epoch: u64,
    ) -> Result<Value, String> {
        if cookie.is_empty() {
            return Err("登录响应中没有会话凭据".into());
        }

        {
            let mut s = self.session.lock().unwrap();
            if epoch != self.epoch.load(Ordering::Acquire) {
                return Err("登录请求已取消".into());
            }
            self.storage.session(&cookie, remember)?;
            s.cookie = cookie;
            s.uid.clear();
        }
        Ok(json!({ "authenticated": true }))
    }
    async fn login(&self, data: &Value, epoch: u64) -> Result<Value, String> {
        let response = self
            .api
            .as_ref()
            .map_err(Clone::clone)?
            .call("login", data["params"].clone(), "")
            .await?
            .checked()?;
        self.authenticate(
            response.session_cookie(),
            data["remember"].as_bool().unwrap_or(false),
            epoch,
        )
        .await
    }
    async fn qr(self: &Arc<Self>, data: &Value, token: u64, epoch: u64) -> Result<Value, String> {
        // Expired keys renew automatically, with one polling request in flight.
        // The owning task is aborted on mode changes, dialog close or refresh.
        loop {
            if epoch != self.epoch.load(Ordering::Acquire) {
                return Ok(Value::Null);
            }
            let body = self.call("login_qr_key", json!({}), "").await?;
            let key = text(&body["data"]["unikey"]);
            if key.is_empty() {
                return Err("加载二维码失败".into());
            }
            let body = self
                .call("login_qr_create", json!({ "key":key, "qrimg":true }), "")
                .await?;
            self.image_loader
                .login_qr(text(&body["data"]["qrimg"]), epoch, token)
                .await?;
            self.event(
                "qr",
                token,
                epoch,
                Ok(json!({ "message":"请使用 App 扫码" })),
            );
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if epoch != self.epoch.load(Ordering::Acquire) {
                    return Ok(Value::Null);
                }
                let response = self
                    .api
                    .as_ref()
                    .map_err(Clone::clone)?
                    .call("login_qr_check", json!({ "key":key }), "")
                    .await?;
                match response.body["code"].as_i64().unwrap_or(0) {
                    803 => {
                        return self
                            .authenticate(
                                response.session_cookie(),
                                data["remember"].as_bool().unwrap_or(true),
                                epoch,
                            )
                            .await
                    }
                    800 => {
                        self.event(
                            "qr",
                            token,
                            epoch,
                            Ok(json!({ "message":"二维码已过期，正在刷新..." })),
                        );
                        break;
                    }
                    801 => {}
                    802 => self.event(
                        "qr",
                        token,
                        epoch,
                        Ok(json!({ "message":"扫描成功！请在手机上确认" })),
                    ),
                    _ => {
                        return Err(response.body["message"]
                            .as_str()
                            .filter(|m| !m.is_empty())
                            .unwrap_or("加载二维码失败")
                            .into())
                    }
                }
            }
        }
    }
    async fn play(
        self: &Arc<Self>,
        data: &Value,
        version: u64,
        request_token: u64,
    ) -> Result<Value, String> {
        let song: Song = serde_json::from_value(data["song"].clone()).map_err(|e| e.to_string())?;
        let start_secs =
            Some(clamp_position(&song, data["position_ms"].as_u64().unwrap_or(0)) as f64 / 1000.0);
        let player = self.player()?;
        let prefs = self.prefs.read().unwrap().clone();
        let (strict, exclusive) = prefs.output_options(prefs.device.as_deref());
        let mut source = None;
        *self.metadata.lock().unwrap() = None;
        if song.path.is_none() {
            let body = self
                .call(
                    "song_url",
                    json!({ "id" : song.id, "level" : prefs.quality }),
                    &self.cookie(),
                )
                .await?;
            let item = &body["data"][0];
            let url = text(&item["url"]);
            if url.is_empty() || item["freeTrialInfo"].is_object() {
                return Err("该歌曲暂无可用的完整音源，请检查会员、购买权限或所选音质".into());
            }
            let id = song.id.parse::<i64>().map_err(|_| "无效的歌曲 ID")?;
            let quality = item["level"].as_str().unwrap_or(&prefs.quality).to_owned();
            source = Some(
                self.cache
                    .prepare_song_source(id, quality, url, item["size"].as_i64())
                    .await
                    .map_err(|e| e.to_string())?,
            );
        }
        let _gate = self.playback_gate.lock().await;
        if version != self.play_version.load(Ordering::Acquire) {
            return Ok(Value::Null);
        }
        let result = if let Some(path) = &song.path {
            player
                .play_file(
                    path.to_string_lossy().into_owned(),
                    start_secs,
                    Some(strict),
                    Some(exclusive),
                )
                .await
        } else {
            let source = source.unwrap();
            *self.metadata.lock().unwrap() = source.metadata_path.clone();
            if source.r#type == "file" {
                player
                    .play_file(source.value, start_secs, Some(strict), Some(exclusive))
                    .await
            } else if let (Some(cache), Some(meta)) = (source.cache_path, source.metadata_path) {
                player
                    .play_url_cached(
                        source.value,
                        cache,
                        meta,
                        Some(song.duration as i64),
                        source.cache_ahead_secs,
                        source.max_cache_ahead_bytes,
                        start_secs,
                        Some(strict),
                        Some(exclusive),
                    )
                    .await
            } else {
                player
                    .play_url(source.value, start_secs, Some(strict), Some(exclusive))
                    .await
            }
        };
        result.map_err(|e| e.to_string())?;
        if version != self.play_version.load(Ordering::Acquire) {
            return Ok(Value::Null);
        }
        self.active_play_token
            .store(request_token, Ordering::Release);
        player
            .set_volume(self.prefs.read().unwrap().effective_volume())
            .map_err(|e| e.to_string())?;
        let services = self.clone();
        tokio::spawn(async move {
            match player.wait_finished().await {
                Ok(reason) if version == services.play_version.load(Ordering::Acquire) => services
                    .event(
                        "finished",
                        request_token,
                        services.epoch.load(Ordering::Acquire),
                        Ok(json!({ "reason" : reason })),
                    ),
                _ => {}
            }
        });
        Ok(json!({ "song" : song }))
    }
    async fn devices(&self) -> Result<Value, String> {
        let devices = self
            .player()?
            .get_output_devices()
            .await
            .map_err(|e| e.to_string())?;
        let prefs = self.prefs.read().unwrap().clone();
        let follow_default = prefs.device.is_none();
        let mut result = vec![
            json!({"id":"","name":"系统默认输出","default":true,"current":follow_default,"aliases":[]}),
        ];
        result.extend(devices.into_iter().map(|d|json!({"id":d.id,"name":d.name,"default":false,"current":!follow_default&&d.is_current,"aliases":d.aliases})));
        Ok(json!(result))
    }
    async fn cache_stats(&self) -> Result<Value, String> {
        let s = self.cache.get_stats().await.map_err(|e| e.to_string())?;
        Ok(json!(
            { "total" : s.total_bytes, "max" : s.max_size_bytes, "song" : s
            .song_bytes, "entity" : s.entity_bytes, "cover" : s.cover_bytes, "lyric"
            : s.lyric_bytes, "song_count" : s.song_entries, "entity_count" : s
            .entity_entries, "cover_count" : s.cover_entries, "lyric_count" : s
            .lyric_entries }
        ))
    }
    async fn mutate(&self, data: &Value, cookie: &str) -> Result<Value, String> {
        if cookie.is_empty() {
            return Err("请先登录".into());
        }
        let _gate = self.mutation_gate.lock().await;
        let op = text(&data["op"]);
        let result = self.call(&op, data["params"].clone(), cookie).await?;
        Ok(json!({ "op" : op, "request" : data, "body" : result }))
    }
    async fn local(&self, data: &Value) -> Result<Value, String> {
        let _gate = self.library_gate.lock().await;
        let action = text(&data["action"]);
        let mut prefs = self.prefs.read().unwrap().clone();
        if action == "remove-track" {
            let songs = library::remove_track(&self.storage, &text(&data["id"]))?;
            return Ok(json!({ "songs": songs, "folders": prefs.folders, "truncated": false }));
        }
        match action.as_str() {
            "add" => {
                let Some(folder) = rfd::AsyncFileDialog::new()
                    .set_title("添加音乐文件夹")
                    .pick_folder()
                    .await
                else {
                    return Ok(Value::Null);
                };
                let path = std::fs::canonicalize(folder.path()).map_err(|e| e.to_string())?;
                if !prefs.folders.contains(&path) {
                    prefs.folders.push(path)
                }
            }
            "remove" => prefs
                .folders
                .retain(|p| p != &PathBuf::from(text(&data["path"]))),
            "clear" => prefs.folders.clear(),
            _ => {}
        }
        let (songs, truncated) = if matches!(action.as_str(), "remove" | "clear") {
            let mut songs: Vec<Song> = self.storage.read("library.json")?;
            songs.retain(|s| s.root.as_ref().is_some_and(|r| prefs.folders.contains(r)));
            self.storage.save("library.json", &songs)?;
            (songs, false)
        } else {
            library::rescan(self.storage.clone(), prefs.folders.clone()).await?
        };
        {
            let mut current = self.prefs.write().unwrap();
            current.folders = prefs.folders.clone();
            self.storage.save("preferences.json", &*current)?;
        }
        Ok(json!(
            { "songs" : songs, "folders" : prefs.folders, "truncated" : truncated }
        ))
    }
    async fn dispatch(self: Arc<Self>, cmd: Command, epoch: u64, play_version: u64) {
        let cookie = self.cookie();
        let result = match cmd.name.as_str() {
            "covers" => {
                self.image_loader.load(&cmd.data, epoch, &self.epoch).await;
                return;
            }
            "home" => self.home(&cookie).await,
            "home-albums" => self.call("album_newest", json!({}), "").await,
            "home-charts" => self.call("toplist_detail", json!({}), "").await,
            "account" => {
                let result = self.account(&cookie, epoch).await;
                if let Ok(profile) = &result {
                    let mut session = self.session.lock().unwrap();
                    if epoch == self.epoch.load(Ordering::Acquire) && session.cookie == cookie {
                        session.uid = text(&profile["body"]["profile"]["userId"]);
                    }
                }
                result
            }
            "page" => self.page(&cmd.data, &cookie, cmd.token, epoch).await,
            "login" => self.login(&cmd.data, epoch).await,
            "qr" => {
                let r = self.qr(&cmd.data, cmd.token, epoch).await;
                self.event("login", 0, epoch, r);
                return;
            }
            "captcha" => self.call("captcha_sent", cmd.data.clone(), "").await,
            "mutation" => self.mutate(&cmd.data, &cookie).await,
            "play" => self.play(&cmd.data, play_version, cmd.token).await,
            "lyrics" => self
                .call("lyric", cmd.data.clone(), &cookie)
                .await
                .map(|b| json!(data::parse_lyrics(&b))),
            "comments" => match self.call("comment_music", cmd.data.clone(), &cookie).await {
                Ok(b) => {
                    let images = json!({});
                    Ok(json!({ "body" : b, "images" : images }))
                }
                Err(e) => Err(e),
            },
            "devices" => self.devices().await,
            "device" => {
                async {
                    let _settings = self.settings_gate.lock().await;
                    if cmd.token != self.device_token.load(Ordering::Acquire) {
                        return Ok(Value::Null);
                    }
                    let id = cmd.data["id"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned);
                    let prefs = self.prefs.read().unwrap().clone();
                    let (strict, exclusive) = prefs.output_options(id.as_deref());
                    self.player()?
                        .switch_output_device(id.clone(), Some(strict), Some(exclusive))
                        .await
                        .map_err(|e| e.to_string())?;
                    {
                        let mut prefs = self.prefs.write().unwrap();
                        prefs.device = id;
                        self.storage.save("preferences.json", &*prefs)?;
                    }
                    self.devices().await
                }
                .await
            }
            "local" => self.local(&cmd.data).await,
            "cache" => {
                if cmd.data["clear"] == true {
                    let _playback = self.playback_gate.lock().await;
                    if let Some(p) = &self.player {
                        let _ = p.stop();
                        let _ = p.wait_finished().await;
                    }
                    *self.metadata.lock().unwrap() = None;
                    let r = self.cache.clear().await.map_err(|e| e.to_string());
                    if let Err(e) = r {
                        self.event("cache", cmd.token, epoch, Err(e));
                        return;
                    }
                }
                self.cache_stats().await
            }
            "download-options" => {
                async {
                    let song: Song = serde_json::from_value(cmd.data["song"].clone())
                        .map_err(|e| e.to_string())?;
                    super::download::quality_options(
                        self.api.as_ref().map_err(Clone::clone)?.as_ref(),
                        &cookie,
                        &song,
                    )
                    .await
                }
                .await
            }
            "download" => {
                let result = super::download::download(
                    self.api.as_ref().map_err(Clone::clone).cloned(),
                    &cookie,
                    &cmd.data,
                )
                .await;
                let mut body = match result {
                    Ok(v) => v,
                    Err(e) => json!({"error":e}),
                };
                body["song_id"] = cmd.data["song"]["id"].clone();
                self.event("download", 0, epoch, Ok(body));
                return;
            }
            "configure-xeapi" => self.call("configure_xeapi", json!({}), &cookie).await,
            "mv" => {
                async {
                    let b = self.call("mv_url", cmd.data.clone(), &cookie).await?;
                    let url = text(&b["data"]["url"]);
                    super::desktop::open_url(&url).await?;
                    Ok(Value::Null)
                }
                .await
            }
            _ => Err(format!("尚未识别的操作: {}", cmd.name)),
        };
        if cmd.name == "play" && play_version != self.play_version.load(Ordering::Acquire) {
            return;
        }
        self.event(
            &cmd.name,
            cmd.token,
            if cmd.name == "play" {
                self.epoch.load(Ordering::Acquire)
            } else {
                epoch
            },
            result,
        );
    }
    fn handle_command(self: &Arc<Self>, cmd: Command) -> bool {
        let epoch = self.epoch.load(Ordering::Acquire);
        match cmd.name.as_str() {
            "shutdown" => {
                if let Some(task) = self.auth_task.lock().unwrap().take() {
                    task.abort();
                }
                return false;
            }
            "logout" | "cancel-login" => {
                if let Some(task) = self.auth_task.lock().unwrap().take() {
                    task.abort();
                }
                let epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
                if cmd.name == "logout" {
                    let cookie = self.cookie();
                    *self.session.lock().unwrap() = Session::default();
                    self.event(
                        "logout",
                        cmd.token,
                        epoch,
                        self.storage.session("", false).map(|_| Value::Null),
                    );
                    let service = self.clone();
                    tokio::spawn(async move {
                        let _ = service.call("logout", json!({}), &cookie).await;
                    });
                }
            }
            "toggle" | "seek" | "stop" => {
                if cmd.name == "stop" {
                    self.active_play_token.store(0, Ordering::Release);
                    self.play_version.fetch_add(1, Ordering::AcqRel);
                }
                let result = self.player().and_then(|player| {
                    match cmd.name.as_str() {
                        "toggle" => {
                            if player.is_playing() {
                                player.pause()
                            } else {
                                player.resume()
                            }
                        }
                        "seek" => player.seek(cmd.data["seconds"].as_f64().unwrap_or(0.0)),
                        _ => player.stop(),
                    }
                    .map_err(|e| e.to_string())
                });
                if let Err(error) = result {
                    self.event("audio-error", 0, epoch, Err(error));
                }
            }
            "save-playback" => {
                // This runs in command order, so shutdown cannot overtake a final save.
                let result = serde_json::from_value::<PlaybackState>(cmd.data)
                    .map_err(|e| e.to_string())
                    .and_then(|saved| self.storage.save("playback.json", &saved));
                if let Err(error) = result {
                    eprintln!("无法保存播放状态: {error}");
                    self.event("save-playback", cmd.token, epoch, Err(error));
                }
            }
            "preferences" => {
                if let Ok(mut next) = serde_json::from_value::<Preferences>(cmd.data) {
                    let old = self.prefs.read().unwrap().clone();
                    // Device and folder changes are committed by their successful native operation.
                    next.folders = old.folders.clone();
                    next.device = old.device.clone();
                    next.normalize();
                    *self.prefs.write().unwrap() = next.clone();
                    if let Err(error) = self.storage.save("preferences.json", &next) {
                        self.event("preferences", 0, epoch, Err(error));
                    }
                    if let Some(player) = &self.player {
                        let _ = player.set_volume(next.effective_volume());
                    }
                    let service = self.clone();
                    tokio::spawn(async move {
                        let _settings = service.settings_gate.lock().await;
                        let result = async {
                            if *service.prefs.read().unwrap() != next {
                                return Ok(Value::Null);
                            }
                            let old = service.applied_prefs.lock().unwrap().clone();
                            if old.visual["auto_launch"] != next.visual["auto_launch"] {
                                super::desktop::autostart(next.visual["auto_launch"] == true)?;
                            }
                            if old.cache_limit_mb != next.cache_limit_mb {
                                service
                                    .cache
                                    .set_max_size_bytes((next.cache_limit_mb * 1024 * 1024) as i64)
                                    .await
                                    .map_err(|e| e.to_string())?;
                            }
                            if old.cache_ahead_mb != next.cache_ahead_mb {
                                service
                                    .cache
                                    .set_song_max_cache_ahead_bytes(
                                        (next.cache_ahead_mb * 1024 * 1024) as i64,
                                    )
                                    .await
                                    .map_err(|e| e.to_string())?;
                            }
                            if old.strict != next.strict || old.exclusive != next.exclusive {
                                let (strict, exclusive) =
                                    next.output_options(next.device.as_deref());
                                service
                                    .player()?
                                    .switch_output_device(
                                        next.device.clone(),
                                        Some(strict),
                                        Some(exclusive),
                                    )
                                    .await
                                    .map_err(|e| e.to_string())?;
                            }
                            *service.applied_prefs.lock().unwrap() = next;
                            service.cache_stats().await
                        }
                        .await;
                        service.event("cache", 0, service.epoch.load(Ordering::Acquire), result);
                    });
                }
            }
            _ => {
                if cmd.name == "device" {
                    self.device_token.store(cmd.token, Ordering::Release);
                }
                let authentication = matches!(cmd.name.as_str(), "login" | "qr");
                if authentication {
                    if let Some(task) = self.auth_task.lock().unwrap().take() {
                        task.abort();
                    }
                    self.epoch.fetch_add(1, Ordering::AcqRel);
                }
                let version = if cmd.name == "play" {
                    self.active_play_token.store(0, Ordering::Release);
                    let version = self.play_version.fetch_add(1, Ordering::AcqRel) + 1;
                    if let Some(player) = &self.player {
                        let _ = player.stop();
                    }
                    version
                } else {
                    self.play_version.load(Ordering::Acquire)
                };
                let epoch = self.epoch.load(Ordering::Acquire);
                let task = tokio::spawn(self.clone().dispatch(cmd, epoch, version));
                if authentication {
                    *self.auth_task.lock().unwrap() = Some(task.abort_handle());
                }
            }
        }
        true
    }
    async fn publish_snapshot(&self) {
        let Some(player) = &self.player else {
            return;
        };
        let play_token = self.active_play_token.load(Ordering::Acquire);
        let snapshot = player.snapshot();
        let metadata = self.metadata.lock().unwrap().clone();
        let mut buffered = 0.0;
        if let Some(metadata) = metadata {
            let _ = self
                .cache
                .update_song_cache_playback_position(metadata.clone(), snapshot.progress_ms as i64);
            if let Ok(progress) = self.cache.get_song_cache_progress(metadata).await {
                buffered = progress.percent;
            }
        }
        self.event(
            "snapshot",
            0,
            self.epoch.load(Ordering::Acquire),
            Ok(json!({
                "progress": snapshot.progress_ms,
                "play_token": play_token,
                "active": matches!(snapshot.status, native::PlaybackStatus::Playing | native::PlaybackStatus::Paused),
                "playing": snapshot.status == native::PlaybackStatus::Playing,
                "paused": snapshot.status == native::PlaybackStatus::Paused,
                "buffering": snapshot.buffering,
                "volume": snapshot.volume,
                "locked": snapshot.volume_locked,
                "buffered": buffered,
            })),
        );
    }
}
pub async fn run(
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: std::sync::mpsc::Sender<Event>,
    images: mpsc::Sender<DecodedImage>,
    storage: Storage,
    prefs: Preferences,
    provider: Option<Arc<dyn MusicApi>>,
) {
    let cache = match native::CacheService::new(
        storage.cache.to_string_lossy().into_owned(),
        Some((prefs.cache_limit_mb * 1024 * 1024) as i64),
    ) {
        Ok(cache) => Arc::new(cache),
        Err(e) => {
            let _ = events.send(Event {
                name: "startup".into(),
                data: Value::Null,
                token: 0,
                epoch: 0,
                error: Some(format!("缓存初始化失败: {e}")),
            });
            return;
        }
    };
    let player = native::PlayerService::new().map(Arc::new);
    let services = Arc::new(Services {
        api: provider.map(Ok).unwrap_or_else(api::configured_provider),
        image_loader: ImageLoader::new(cache.clone(), images),
        cache,
        player: player.as_ref().ok().cloned(),
        storage,
        prefs: RwLock::new(prefs.clone()),
        applied_prefs: Mutex::new(prefs.clone()),
        session: Mutex::new(Session::default()),
        epoch: AtomicU64::new(0),
        auth_task: Mutex::new(None),
        play_version: AtomicU64::new(0),
        active_play_token: AtomicU64::new(0),
        playback_gate: tokio::sync::Mutex::new(()),
        mutation_gate: tokio::sync::Mutex::new(()),
        library_gate: tokio::sync::Mutex::new(()),
        settings_gate: tokio::sync::Mutex::new(()),
        device_token: AtomicU64::new(0),
        metadata: Mutex::new(None),
        events,
    });
    if let Err(e) = player {
        services.event("audio-error", 0, 0, Err(format!("音频设备初始化失败: {e}")))
    }
    match services.storage.read::<String>("session.json") {
        Ok(cookie) => services.session.lock().unwrap().cookie = cookie,
        Err(e) => services.event("startup", 0, 0, Err(e)),
    }
    let _ = services
        .cache
        .set_max_size_bytes((prefs.cache_limit_mb * 1024 * 1024) as i64)
        .await;
    let _ = services
        .cache
        .set_song_max_cache_ahead_bytes((prefs.cache_ahead_mb * 1024 * 1024) as i64)
        .await;
    if let Some(p) = &services.player {
        let _ = p.set_volume(prefs.effective_volume());
        if prefs.device.is_some() {
            let (strict, exclusive) = prefs.output_options(prefs.device.as_deref());
            if let Err(e) = p
                .switch_output_device(prefs.device.clone(), Some(strict), Some(exclusive))
                .await
            {
                services.prefs.write().unwrap().device = None;
                services.event("audio-error", 0, 0, Err(format!("已回退系统默认输出: {e}")))
            }
        }
    }
    let library_storage = services.storage.clone();
    let songs = tokio::task::spawn_blocking(move || library::load_existing(&library_storage))
        .await
        .map_err(|e| e.to_string())
        .and_then(|v| v);
    services.event(
        "local",
        0,
        0,
        songs.map(|songs| json!({ "songs" : songs, "folders" : prefs.folders })),
    );
    let has_cookie = !services.cookie().is_empty();
    for name in if has_cookie {
        vec!["home", "account", "devices", "cache"]
    } else {
        vec!["home", "devices", "cache"]
    } {
        tokio::spawn(services.clone().dispatch(
            Command {
                name: name.into(),
                data: json!({}),
                token: 0,
            },
            0,
            0,
        ));
    }
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break; };
                if !services.handle_command(command) { break; }
            }
            _ = tick.tick() => services.publish_snapshot().await,
        }
    }
    if let Some(p) = &services.player {
        let _ = p.stop();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::api::{ApiFuture, ApiResponse};

    fn login_services(
        api: Arc<dyn MusicApi>,
    ) -> (
        Arc<Services>,
        std::sync::mpsc::Receiver<Event>,
        mpsc::Receiver<DecodedImage>,
    ) {
        let path = crate::runtime::tests::temp("login-backend");
        let storage = Storage {
            config: path.clone(),
            cache: path.join("cache"),
        };
        let cache = Arc::new(
            native::CacheService::new(storage.cache.to_string_lossy().into_owned(), None).unwrap(),
        );
        let (events, receiver) = std::sync::mpsc::channel();
        let (images, image_receiver) = mpsc::channel(8);
        (
            Arc::new(Services {
                api: Ok(api),
                image_loader: ImageLoader::new(cache.clone(), images),
                cache,
                player: None,
                storage,
                prefs: RwLock::new(Preferences::default()),
                applied_prefs: Mutex::new(Preferences::default()),
                session: Mutex::new(Session::default()),
                epoch: AtomicU64::new(0),
                auth_task: Mutex::new(None),
                play_version: AtomicU64::new(0),
                active_play_token: AtomicU64::new(0),
                playback_gate: tokio::sync::Mutex::new(()),
                mutation_gate: tokio::sync::Mutex::new(()),
                library_gate: tokio::sync::Mutex::new(()),
                settings_gate: tokio::sync::Mutex::new(()),
                device_token: AtomicU64::new(0),
                metadata: Mutex::new(None),
                events,
            }),
            receiver,
            image_receiver,
        )
    }

    struct DiscoveryApi;
    impl MusicApi for DiscoveryApi {
        fn call(&self, operation: &str, _: Value, cookie: &str) -> ApiFuture<'_> {
            assert!(
                cookie.is_empty(),
                "public discovery should not depend on login"
            );
            let result = match operation {
                "album_newest" => Ok(ApiResponse {
                    status: 200,
                    body: json!({"code":200,"albums":[{"id":42,"name":"New album"}]}),
                    cookie: vec![],
                }),
                "toplist_detail" => Err("chart unavailable".to_owned()),
                _ => panic!("unexpected endpoint {operation}"),
            };
            Box::pin(async move { result })
        }
    }

    #[tokio::test]
    async fn public_discovery_endpoints_publish_independent_results() {
        let (services, receiver, _) = login_services(Arc::new(DiscoveryApi));
        services.session.lock().unwrap().cookie = "private-session".into();
        tokio::join!(
            services.clone().dispatch(
                Command {
                    name: "home-albums".into(),
                    data: json!({}),
                    token: 7
                },
                0,
                0
            ),
            services.clone().dispatch(
                Command {
                    name: "home-charts".into(),
                    data: json!({}),
                    token: 9
                },
                0,
                0
            ),
        );
        let events = [receiver.try_recv().unwrap(), receiver.try_recv().unwrap()];
        let albums = events.iter().find(|e| e.name == "home-albums").unwrap();
        assert_eq!(albums.token, 7);
        assert!(albums.error.is_none());
        assert_eq!(albums.data["albums"][0]["id"], 42);
        assert!(events
            .iter()
            .find(|e| e.name == "home-charts")
            .unwrap()
            .error
            .is_some());
        std::fs::remove_dir_all(&services.storage.config).unwrap();
    }

    struct QrApi {
        png: String,
        calls: Mutex<Vec<(String, Value)>>,
        checks: AtomicU64,
    }
    impl MusicApi for QrApi {
        fn call(&self, op: &str, params: Value, _: &str) -> ApiFuture<'_> {
            self.calls.lock().unwrap().push((op.into(), params));
            let body = match op {
                "login_qr_key" => {
                    json!({"code":200,"data":{"unikey":format!("key-{}",self.checks.load(Ordering::Acquire))}})
                }
                "login_qr_create" => json!({"code":200,"data":{"qrimg":self.png}}),
                "login_qr_check" => match self.checks.fetch_add(1, Ordering::AcqRel) {
                    0 => json!({"code":800}),
                    1 => json!({"code":801}),
                    2 => json!({"code":802}),
                    _ => json!({"code":803,"cookie":"qr-cookie"}),
                },
                _ => panic!("authentication must not wait for account/library requests: {op}"),
            };
            Box::pin(async move {
                Ok(ApiResponse {
                    status: 200,
                    body,
                    cookie: vec![],
                })
            })
        }
    }

    #[tokio::test]
    async fn qr_refresh_persists_by_default_without_waiting_for_account_and_restores_on_restart() {
        use base64::Engine;
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let api = Arc::new(QrApi {
            png: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
            ),
            calls: Mutex::new(vec![]),
            checks: AtomicU64::new(0),
        });
        let (service, events, mut images) = login_services(api.clone());
        let result = tokio::time::timeout(Duration::from_secs(12), service.qr(&json!({}), 9, 0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result["authenticated"], true);
        assert_eq!(service.cookie(), "qr-cookie");
        assert_eq!(
            service.storage.read::<String>("session.json").unwrap(),
            "qr-cookie"
        );
        assert_eq!(
            api.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(op, _)| op == "login_qr_key")
                .count(),
            2
        );
        let messages: Vec<_> = events
            .try_iter()
            .map(|e| text(&e.data["message"]))
            .collect();
        assert_eq!(
            messages,
            vec![
                "请使用 App 扫码",
                "二维码已过期，正在刷新...",
                "请使用 App 扫码",
                "扫描成功！请在手机上确认"
            ]
        );
        for _ in 0..2 {
            let image = images.try_recv().unwrap();
            assert!(matches!(
                image.target,
                super::super::images::ImageTarget::LoginQr(9)
            ));
            assert_eq!(image.epoch, 0);
        }
        assert!(images.try_recv().is_err());
        let path = service.storage.config.clone();
        let storage = service.storage.clone();
        drop(service);

        // A fresh runtime must use the saved session without another login.
        let api = Arc::new(MockApi::default());
        let (sender, commands) = mpsc::unbounded_channel();
        let (events, _receiver) = std::sync::mpsc::channel();
        let (images, _image_receiver) = mpsc::channel(8);
        let runtime = tokio::spawn(run(
            commands,
            events,
            images,
            storage.clone(),
            Preferences::default(),
            Some(api.clone()),
        ));
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if api
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(op, _, cookie)| op == "user_account" && cookie == "qr-cookie")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("startup must restore the saved QR session");
        sender
            .send(Command {
                name: "logout".into(),
                data: Value::Null,
                token: 1,
            })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while storage.config.join("session.json").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("logout must remove the saved QR session");
        sender
            .send(Command {
                name: "shutdown".into(),
                data: Value::Null,
                token: 0,
            })
            .unwrap();
        runtime.await.unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[tokio::test]
    async fn closing_login_aborts_inflight_authentication_without_saving_a_session() {
        use std::sync::atomic::AtomicBool;
        struct BlockedApi {
            started: AtomicBool,
            dropped: Arc<AtomicBool>,
        }
        impl MusicApi for BlockedApi {
            fn call(&self, op: &str, _: Value, _: &str) -> ApiFuture<'_> {
                assert_eq!(op, "login");
                struct Guard(Arc<AtomicBool>);
                impl Drop for Guard {
                    fn drop(&mut self) {
                        self.0.store(true, Ordering::Release);
                    }
                }
                let dropped = self.dropped.clone();
                Box::pin(async move {
                    let _guard = Guard(dropped);
                    self.started.store(true, Ordering::Release);
                    std::future::pending::<()>().await;
                    unreachable!()
                })
            }
        }
        let api = Arc::new(BlockedApi {
            started: AtomicBool::new(false),
            dropped: Arc::new(AtomicBool::new(false)),
        });
        let (service, events, _images) = login_services(api.clone());
        service.handle_command(Command {
            name: "login".into(),
            data: json!({"params":{},"remember":true}),
            token: 1,
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while !api.started.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        service.handle_command(Command {
            name: "cancel-login".into(),
            data: Value::Null,
            token: 1,
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while !api.dropped.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(service.cookie().is_empty());
        assert!(!service.storage.config.join("session.json").exists());
        assert!(events.try_recv().is_err());
        // A successful remembered session is saved only after the explicit choice.
        service
            .authenticate(
                "remembered".into(),
                true,
                service.epoch.load(Ordering::Acquire),
            )
            .await
            .unwrap();
        assert_eq!(
            service.storage.read::<String>("session.json").unwrap(),
            "remembered"
        );
        let path = service.storage.config.clone();
        drop(service);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[derive(Default)]
    struct MockApi {
        calls: Mutex<Vec<(String, Value, String)>>,
        hold_song_details: bool,
        song_details_started: tokio::sync::Notify,
        release_song_details: tokio::sync::Notify,
    }
    impl MusicApi for MockApi {
        fn call(&self, op: &str, params: Value, cookie: &str) -> ApiFuture<'_> {
            self.calls
                .lock()
                .unwrap()
                .push((op.into(), params.clone(), cookie.into()));
            let body = match op {
                "playlist_detail" => {
                    json!(
                        { "code" : 200, "playlist" : { "name": "test playlist", "trackCount": 2, "trackIds" : [{ "id" :
                        4500000001u64 }, { "id" : 4500000002u64 }], "tracks" : [{ "id" :
                        4500000001u64 }] } }
                    )
                }
                "song_detail" => {
                    json!(
                        { "code" : 200, "songs" : [{ "id" : 4500000001u64, "name" : "one"
                        }, { "id" : 4500000002u64, "name" : "two" }] }
                    )
                }
                "user_account" => {
                    json!(
                        { "code" : 200, "profile" : { "userId" : 6500000001u64,
                        "nickname" : "test" } }
                    )
                }
                "user_playlist" => json!({ "code" : 200, "playlist" : [] }),
                "likelist" => json!({ "code" : 200, "ids" : [] }),
                "lyric" => {
                    json!({ "code" : 200, "lrc" : { "lyric" : "[00:01.00]test" } })
                }
                "playlist_update" => {
                    json!({ "code" : 500, "message" : "partial update rejected" })
                }
                _ => json!({ "code" : 200 }),
            };
            let hold = self.hold_song_details && op == "song_detail";
            Box::pin(async move {
                if hold {
                    self.song_details_started.notify_one();
                    self.release_song_details.notified().await;
                }
                Ok(ApiResponse {
                    status: 200,
                    body,
                    cookie: vec![],
                })
            })
        }
    }
    #[tokio::test]
    async fn injected_rust_provider_covers_playlist_pagination_auth_mutations_and_account_scoped_cache(
    ) {
        let dir = std::env::temp_dir().join(format!("ncm-services-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = Storage {
            config: dir.clone(),
            cache: dir.join("cache"),
        };
        let api = Arc::new(MockApi {
            hold_song_details: true,
            ..Default::default()
        });
        let (tx, events) = std::sync::mpsc::channel();
        let cache = Arc::new(
            native::CacheService::new(
                storage.cache.to_string_lossy().into_owned(),
                Some(16 * 1024 * 1024),
            )
            .unwrap(),
        );
        let service = Services {
            api: Ok(api.clone()),
            image_loader: ImageLoader::new(cache.clone(), mpsc::channel(8).0),
            cache,
            player: None,
            storage: storage.clone(),
            prefs: RwLock::new(Preferences::default()),
            applied_prefs: Mutex::new(Preferences::default()),
            session: Mutex::new(Session::default()),
            epoch: AtomicU64::new(1),
            auth_task: Mutex::new(None),
            play_version: AtomicU64::new(0),
            active_play_token: AtomicU64::new(0),
            playback_gate: tokio::sync::Mutex::new(()),
            mutation_gate: tokio::sync::Mutex::new(()),
            library_gate: tokio::sync::Mutex::new(()),
            settings_gate: tokio::sync::Mutex::new(()),
            device_token: AtomicU64::new(0),
            metadata: Mutex::new(None),
            events: tx,
        };
        let request = json!({ "kind" : "playlist", "id" : "7500000001" });
        let page = {
            let page = service.page(&request, "account-a", 1, 1);
            tokio::pin!(page);
            tokio::time::timeout(Duration::from_secs(2), async {
                tokio::select! {
                    result = &mut page => panic!("page completed before song details: {result:?}"),
                    _ = api.song_details_started.notified() => {}
                }
            })
            .await
            .unwrap();
            let metadata = events.try_recv().unwrap();
            assert_eq!(metadata.name, "playlist-metadata");
            assert_eq!((metadata.token, metadata.epoch), (1, 1));
            assert_eq!(metadata.data["id"], "7500000001");
            assert_eq!(metadata.data["playlist"]["name"], "test playlist");
            assert!(metadata.data["playlist"].get("tracks").is_none());
            assert!(metadata.data["playlist"].get("trackIds").is_none());
            api.release_song_details.notify_one();
            page.await.unwrap()
        };
        assert_eq!(items(&page["body"]["playlist"]["tracks"]).len(), 2);
        let calls = api.calls.lock().unwrap().clone();
        assert_eq!(calls[1].1["ids"], "4500000001,4500000002");
        service
            .authenticate("account-a".into(), true, 1)
            .await
            .unwrap();
        assert_eq!(storage.read::<String>("session.json").unwrap(), "account-a");
        service.epoch.store(2, Ordering::Release);
        assert!(service.authenticate("stale".into(), true, 1).await.is_err());
        assert_eq!(service.cookie(), "account-a");
        let result = service
            .mutate(
                &json!({ "op" : "playlist_update", "params" : { "id" : "7500000001" } }),
                "account-a",
            )
            .await;
        assert_eq!(result.unwrap_err(), "partial update rejected");
        assert_eq!(
            api.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(op, _, _)| op == "playlist_update")
                .count(),
            1
        );
        for cookie in ["account-a", "account-a", "account-b"] {
            service
                .call("lyric", json!({ "id" : "4500000001" }), cookie)
                .await
                .unwrap();
        }
        assert_eq!(
            api.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(op, _, _)| op == "lyric")
                .count(),
            2
        );
        drop(service);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
