use super::*;
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::{JoinHandle, JoinSet},
};

#[derive(Debug)]
struct Request {
    path: String,
    headers: String,
    body: String,
}
impl Request {
    fn eapi_data(&self) -> Value {
        let params = self.body.strip_prefix("params=").unwrap();
        ncm_api_rs::crypto::eapi_req_decrypt(params).unwrap().1
    }
}

struct Reply {
    body: Value,
    cookies: Vec<&'static str>,
    delay: Duration,
}
impl Reply {
    fn new(body: Value) -> Self {
        Self {
            body,
            cookies: vec![],
            delay: Duration::ZERO,
        }
    }
}

struct Server {
    domain: String,
    requests: mpsc::UnboundedReceiver<Request>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn new(reply: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let domain = format!("http://{}", listener.local_addr().unwrap());
        let (sent, requests) = mpsc::unbounded_channel();
        let reply = Arc::new(reply);
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        let (stream, _) = result.unwrap();
                        let reply = reply.clone();
                        let sent = sent.clone();
                        connections.spawn(async move {
                            serve(stream, reply.as_ref(), sent).await;
                        });
                    }
                    Some(result) = connections.join_next() => { result.unwrap(); }
                }
            }
        });
        Self {
            domain,
            requests,
            task,
        }
    }

    fn api(&self) -> NativeApi {
        let mut api = NativeApi::new();
        api.domain = Some(self.domain.clone());
        api
    }

    async fn next(&mut self) -> Request {
        tokio::time::timeout(Duration::from_secs(2), self.requests.recv())
            .await
            .unwrap()
            .unwrap()
    }
}

async fn serve(
    mut stream: TcpStream,
    reply: &(dyn Fn(&Request) -> Reply + Send + Sync),
    sent: mpsc::UnboundedSender<Request>,
) {
    let mut bytes = Vec::new();
    let end = loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let length: usize = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .unwrap()
        .1
        .trim()
        .parse()
        .unwrap();
    while bytes.len() < end + length {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
    }
    let request = Request {
        path: headers.split_whitespace().nth(1).unwrap().to_owned(),
        headers,
        body: String::from_utf8(bytes[end..end + length].to_vec()).unwrap(),
    };
    let response = reply(&request);
    sent.send(request).unwrap();
    tokio::time::sleep(response.delay).await;
    let body = response.body.to_string();
    let cookies = response
        .cookies
        .iter()
        .map(|cookie| format!("Set-Cookie: {cookie}\r\n"))
        .collect::<String>();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{cookies}Connection: close\r\n\r\n{body}", body.len()
    );
    // A timed-out/cancelled caller may have already closed its connection.
    let _ = stream.write_all(response.as_bytes()).await;
}

#[tokio::test]
async fn operations_reach_the_expected_sdk_endpoints() {
    let mut server = Server::new(|_| Reply::new(json!({"code":200}))).await;
    let api = server.api();
    let operations = [
        ("login", "/weapi/w/login/cellphone"),
        ("captcha_sent", "/weapi/sms/captcha/sent"),
        ("banner", "/eapi/v2/banner/get"),
        ("user_cloud", "/weapi/v1/cloud/get"),
        ("search", "/eapi/cloudsearch/pc"),
        ("song_detail", "/weapi/v3/song/detail"),
        ("song_download_detail", "/weapi/v3/song/detail"),
        ("login_qr_key", "/eapi/login/qrcode/unikey"),
        ("login_qr_check", "/eapi/login/qrcode/client/login"),
        ("user_account", "/weapi/nuser/account/get"),
        ("playlist_catlist", "/eapi/playlist/catalogue"),
        ("user_playlist", "/weapi/user/playlist"),
        ("playlist_detail", "/eapi/v6/playlist/detail"),
        ("lyric", "/eapi/song/lyric/v1"),
        (
            "recommend_resource",
            "/weapi/v1/discovery/recommend/resource",
        ),
        ("recommend_songs", "/weapi/v3/discovery/recommend/songs"),
        ("personalized", "/weapi/personalized/playlist"),
        ("personalized_newsong", "/weapi/personalized/newsong"),
        ("like", "/weapi/radio/like"),
        ("likelist", "/eapi/song/like/get"),
        (
            "comment_music",
            "/weapi/v1/resource/comments/R_SO_4_9007199254740991",
        ),
        ("playlist_create", "/weapi/playlist/create"),
        ("playlist_delete", "/weapi/playlist/remove"),
        ("song_url", "/eapi/song/enhance/player/url/v1"),
        ("song_download_url", "/eapi/song/enhance/download/url/v1"),
        ("artist_detail", "/eapi/artist/head/info/get"),
        ("artist_top_song", "/weapi/artist/top/song"),
        ("artist_album", "/weapi/artist/albums/9007199254740991"),
        ("artist_mv", "/weapi/artist/mvs"),
        ("album", "/weapi/v1/album/9007199254740991"),
        ("logout", "/eapi/logout"),
        ("mv_url", "/weapi/song/enhance/play/mv/url"),
        ("album_newest", "/weapi/discovery/newAlbum"),
        ("toplist_detail", "/weapi/toplist/detail"),
    ];
    for (operation, path) in operations {
        api.call(
            operation,
            json!({"id":"9007199254740991", "cookie":"MUSIC_U=override"}),
            "MUSIC_U=session",
        )
        .await
        .unwrap()
        .checked()
        .unwrap();
        let request = server.next().await;
        assert_eq!(request.path, path, "{operation}");
        assert!(request.headers.contains("MUSIC_U=session"), "{operation}");
        assert!(!request.headers.contains("override"), "{operation}");
    }
    for operation in ["ping", "configure_xeapi"] {
        let response = api
            .call(operation, Value::Null, "")
            .await
            .unwrap()
            .checked()
            .unwrap();
        if operation == "ping" {
            assert_eq!(response.body["provider"], "rust");
        }
    }
    assert!(api
        .call("server", json!({}), "")
        .await
        .unwrap()
        .checked()
        .is_err());
    assert!(server.requests.try_recv().is_err());
}

#[tokio::test]
async fn playlist_writes_preserve_full_ids_unicode_and_partial_failures() {
    let mut server = Server::new(|request| {
        let data = request.eapi_data();
        let body = if request.path == "/eapi/batch" {
            assert!(data.get("/api/playlist/tags/update").is_none());
            let name: Value =
                serde_json::from_str(data[UPDATE_PATHS[0]].as_str().unwrap()).unwrap();
            let desc: Value =
                serde_json::from_str(data[UPDATE_PATHS[1]].as_str().unwrap()).unwrap();
            assert_eq!(
                name,
                json!({"id":"9007199254740991", "name":"名字\"\n新行"})
            );
            let mut body = json!({"code":200, UPDATE_PATHS[0]:{"code":200}});
            match desc["desc"].as_str().unwrap() {
                "line\nline" => body[UPDATE_PATHS[1]] = json!({"code":200}),
                "fail" => body[UPDATE_PATHS[1]] = json!({"code":401,"message":"简介保存失败"}),
                "missing" => {}
                value => panic!("unexpected description: {value}"),
            }
            body
        } else {
            json!({"code":200})
        };
        Reply::new(body)
    })
    .await;
    let api = server.api();
    for (operation, op) in [
        ("playlist_track_add", "add"),
        ("playlist_track_delete", "del"),
    ] {
        api.call(operation, json!({
            "pid":"9007199254740991", "tracks":["4500000001","9007199254740990"], "op":"override"
        }), "MUSIC_U=session").await.unwrap().checked().unwrap();
        let request = server.next().await;
        assert_eq!(request.path, "/eapi/playlist/manipulate/tracks");
        let data = request.eapi_data();
        assert_eq!(data["op"], op);
        assert_eq!(data["pid"], "9007199254740991");
        assert_eq!(
            serde_json::from_str::<Value>(data["trackIds"].as_str().unwrap()).unwrap(),
            json!(["4500000001", "9007199254740990"])
        );
    }
    for desc in ["line\nline", "fail", "missing"] {
        let response = api.call("playlist_update", json!({
            "id":"9007199254740991", "name":"名字\"\n新行", "desc":desc, "tags":"must not change"
        }), "MUSIC_U=session").await.unwrap();
        let result = response.checked();
        assert_eq!(result.is_ok(), desc == "line\nline");
        if desc == "fail" {
            assert_eq!(result.unwrap_err(), "简介保存失败");
        }
        assert_eq!(server.next().await.path, "/eapi/batch");
    }
    assert!(
        server.requests.try_recv().is_err(),
        "mutations must not retry"
    );
}

#[tokio::test]
async fn qr_login_wraps_keys_renders_png_and_keeps_polling_codes_and_cookies() {
    let mut server = Server::new(|request| {
        let mut reply = if request.path.ends_with("/unikey") {
            Reply::new(json!({"code":200,"unikey":"test-key"}))
        } else {
            let key = request.eapi_data()["key"]
                .as_str()
                .unwrap()
                .parse::<i64>()
                .unwrap();
            Reply::new(json!({"code":key}))
        };
        if reply.body["code"] == 803 {
            reply.cookies = vec![
                "MUSIC_U=qr-session; Path=/; HttpOnly",
                "__csrf=token; Path=/",
            ];
        }
        reply
    })
    .await;
    let api = server.api();
    let key = api
        .call("login_qr_key", json!({}), "")
        .await
        .unwrap()
        .checked()
        .unwrap();
    assert_eq!(key.body["data"]["unikey"], "test-key");
    server.next().await;
    for generate in [false, true] {
        let response = api
            .call(
                "login_qr_create",
                json!({"key":"test-key","qrimg":generate}),
                "",
            )
            .await
            .unwrap()
            .checked()
            .unwrap();
        assert_eq!(
            response.body["data"]["qrurl"],
            "https://music.163.com/login?codekey=test-key"
        );
        let encoded = response.body["data"]["qrimg"].as_str().unwrap();
        if generate {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded.strip_prefix("data:image/png;base64,").unwrap())
                .unwrap();
            let image = image::load_from_memory(&bytes).unwrap().into_luma8();
            assert!(image.width() >= 256);
            assert_eq!(image.width(), image.height());
            assert_eq!(
                image.get_pixel(0, 0).0,
                [255],
                "QR quiet zone must remain white"
            );
            assert!(image.pixels().any(|pixel| pixel.0 == [0]));
        } else {
            assert!(encoded.is_empty());
        }
    }
    for code in 800..=803 {
        let response = api
            .call("login_qr_check", json!({"key":code.to_string()}), "")
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body["code"], code);
        if code == 803 {
            assert_eq!(
                response.session_cookie(),
                "MUSIC_U=qr-session; __csrf=token"
            );
        }
        server.next().await;
    }
    assert!(
        server.requests.try_recv().is_err(),
        "QR rendering must stay local"
    );
}

#[tokio::test]
async fn concurrent_requests_keep_sessions_and_pagination_separate() {
    let mut server = Server::new(|request| {
        let data = request.eapi_data();
        let mut reply = Reply::new(json!({"code":200,"query":data}));
        if data["s"] == "slow" {
            reply.delay = Duration::from_millis(60);
        }
        reply.cookies = vec!["MUSIC_U=must-not-leak; Path=/"];
        reply
    })
    .await;
    let api = server.api();
    let (slow, fast) = tokio::join!(
        api.call(
            "search",
            json!({"keywords":"slow","limit":100,"offset":200,"type":1}),
            "MUSIC_U=first"
        ),
        api.call(
            "search",
            json!({"keywords":"fast","limit":20,"offset":0,"type":10}),
            "MUSIC_U=second"
        )
    );
    for (response, term, cookie, limit, offset, kind) in [
        (slow.unwrap(), "slow", "first", 100, 200, 1),
        (fast.unwrap(), "fast", "second", 20, 0, 10),
    ] {
        let data = &response.body["query"];
        assert_eq!(data["s"], term);
        assert_eq!(data["header"]["MUSIC_U"], cookie);
        assert_eq!(data["limit"], limit);
        assert_eq!(data["offset"], offset);
        assert_eq!(data["type"], kind);
        server.next().await;
    }
    api.call("search", json!({"keywords":"anonymous"}), "")
        .await
        .unwrap();
    let request = server.next().await;
    assert!(
        !request.headers.contains("MUSIC_U"),
        "a response must not change the client's session"
    );
}

#[tokio::test]
async fn sdk_special_statuses_are_not_mistaken_for_success() {
    let mut server = Server::new(|request| {
        let code = request.eapi_data()["s"].clone();
        Reply::new(json!({"code":code,"message":"服务拒绝请求","detail":{"reason":"retained"}}))
    })
    .await;
    let api = server.api();
    for code in ["201", "400", "502", "803"] {
        let response = api
            .call("search", json!({"keywords":code}), "")
            .await
            .unwrap();
        assert_eq!(
            response.status, 200,
            "SDK uses 200 for special business statuses"
        );
        assert_eq!(response.body["code"], code.parse::<i64>().unwrap());
        assert_eq!(response.body["detail"]["reason"], "retained");
        assert_eq!(response.checked().unwrap_err(), "服务拒绝请求");
        server.next().await;
    }
}

#[tokio::test]
async fn errors_timeouts_and_cancellation_do_not_retry_or_leak_capacity() {
    let mut server = Server::new(|request| {
        let data = if request.path.starts_with("/eapi/") {
            request.eapi_data()
        } else {
            Value::Null
        };
        match data["level"].as_str().unwrap_or("") {
            "exhigh" => Reply::new(json!({"code":403,"msg":"not purchased"})),
            "lossless" => Reply::new(json!({"code":301,"msg":"login expired"})),
            "hires" => Reply::new(json!({"code":503,"msg":"try later"})),
            _ => Reply {
                delay: Duration::from_secs(2),
                ..Reply::new(json!({"code":200}))
            },
        }
    })
    .await;
    let mut api = server.api();
    api.timeout = Duration::from_millis(150);
    for (level, code, message) in [
        ("exhigh", 403, "not purchased"),
        ("lossless", 301, "login expired"),
        ("hires", 503, "try later"),
    ] {
        let response = api
            .call(
                "song_url",
                json!({"id":"9007199254740991","level":level}),
                "",
            )
            .await
            .unwrap();
        assert_eq!(response.body["code"], code);
        assert_eq!(response.checked().unwrap_err(), message);
        assert_eq!(server.next().await.path, "/eapi/song/enhance/player/url/v1");
    }
    assert!(api
        .call("playlist_create", json!({"name":"timeout"}), "")
        .await
        .unwrap_err()
        .contains("超时"));
    server.next().await;
    assert_eq!(api.in_flight.available_permits(), 64);
    let api = Arc::new(api);
    let cancelled = api.clone();
    let task = tokio::spawn(async move { cancelled.call("song_url", json!({"id":"1"}), "").await });
    server.next().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(api.in_flight.available_permits(), 64);
    let permits = api.in_flight.acquire_many(64).await.unwrap();
    assert!(api
        .call("song_url", json!({"id":"1"}), "")
        .await
        .unwrap_err()
        .contains("请求过多"));
    drop(permits);
    assert!(server.requests.try_recv().is_err());
}

#[tokio::test]
#[ignore = "requires access to the public NetEase API; does not use an account"]
async fn live_native_api_smoke() {
    let api = NativeApi::new();
    for (operation, params, field) in [
        ("search", json!({"keywords":"海阔天空","limit":1}), "result"),
        ("song_detail", json!({"ids":"347230"}), "songs"),
        ("album_newest", json!({}), "albums"),
        ("toplist_detail", json!({}), "list"),
        (
            "song_url",
            json!({"id":"347230","level":"standard"}),
            "data",
        ),
        ("login_qr_key", json!({}), "data"),
    ] {
        let response = api
            .call(operation, params, "")
            .await
            .unwrap_or_else(|error| panic!("{operation}: {error}"))
            .checked()
            .unwrap_or_else(|error| panic!("{operation}: {error}"));
        assert!(
            !response.body[field].is_null(),
            "{operation}: missing {field}"
        );
    }
}
