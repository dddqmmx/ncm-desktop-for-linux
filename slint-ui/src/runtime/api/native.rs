//! Adapts the SDK to the application's stable operations and response shapes.
use super::{ApiFuture, ApiResponse, MusicApi};
use base64::Engine;
use ncm_api_rs::{ApiClient, NcmError, Query};
use serde_json::{json, Value};
use std::{
    io::Cursor,
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const UPDATE_PATHS: [&str; 2] = ["/api/playlist/update/name", "/api/playlist/desc/update"];
const TIMEOUT_MESSAGE: &str = "API 请求超时；操作可能已执行，请刷新确认";
const NETWORK_MESSAGE: &str = "网易云网络连接失败，请检查网络或 API 服务配置";

pub(super) struct NativeApi {
    client: ApiClient,
    in_flight: Semaphore,
    timeout: Duration,
    #[cfg(test)]
    domain: Option<String>,
}

impl NativeApi {
    pub(super) fn new() -> Self {
        Self {
            // The SDK client has no cookie jar. Each request supplies its own session,
            // including an explicit empty cookie for anonymous/login requests.
            client: ApiClient::new(None),
            in_flight: Semaphore::new(64),
            timeout: Duration::from_secs(40),
            #[cfg(test)]
            domain: None,
        }
    }

    async fn dispatch(&self, operation: &str, query: &Query) -> Result<ApiResponse, String> {
        let response = match operation {
            "ping" => {
                return Ok(local_response(
                    json!({"code":200,"provider":"rust","version":1}),
                ))
            }
            // Compatibility with the existing settings command. The SDK uses EAPI
            // directly and does not need Node's generated XEAPI public key config.
            "configure_xeapi" => return Ok(local_response(json!({"code":200}))),
            "login" => self.client.login_cellphone(query).await,
            "captcha_sent" => self.client.captcha_sent(query).await,
            "login_qr_key" => self.client.login_qr_key(query).await,
            "login_qr_create" => self.client.login_qr_create(query).await,
            "login_qr_check" => self.client.login_qr_check(query).await,
            "user_account" => self.client.user_account(query).await,
            "logout" => self.client.logout(query).await,
            "banner" => self.client.banner(query).await,
            "user_cloud" => self.client.user_cloud(query).await,
            "search" => self.client.cloudsearch(query).await,
            "song_detail" | "song_download_detail" => self.client.song_detail(query).await,
            "playlist_catlist" => self.client.playlist_catlist(query).await,
            "user_playlist" => self.client.user_playlist(query).await,
            "playlist_detail" => self.client.playlist_detail(query).await,
            "recommend_resource" => self.client.recommend_resource(query).await,
            "recommend_songs" => self.client.recommend_songs(query).await,
            "personalized" => self.client.personalized(query).await,
            "personalized_newsong" => self.client.personalized_newsong(query).await,
            "album_newest" => self.client.album_newest(query).await,
            "toplist_detail" => self.client.toplist_detail(query).await,
            "song_url" => self.client.song_url_v1(query).await,
            "song_download_url" => self.client.song_download_url_v1(query).await,
            "lyric" => self.client.lyric_new(query).await,
            "like" => self.client.like(query).await,
            "likelist" => self.client.likelist(query).await,
            "comment_music" => self.client.comment_music(query).await,
            "playlist_track_add" | "playlist_track_delete" => {
                self.client.playlist_tracks(query).await
            }
            "playlist_create" => self.client.playlist_create(query).await,
            "playlist_delete" => self.client.playlist_delete(query).await,
            "playlist_update" => self.client.batch(query).await,
            "artist_detail" => self.client.artist_detail(query).await,
            "artist_top_song" => self.client.artist_top_song(query).await,
            "artist_album" => self.client.artist_album(query).await,
            "artist_mv" => self.client.artist_mv(query).await,
            "album" => self.client.album(query).await,
            "mv_url" => self.client.mv_url(query).await,
            _ => return Ok(rejection(400, "不支持的 API 操作")),
        };
        let mut response = adapt_response(response)?;
        if response.status == 200 && response.body["code"].as_i64().unwrap_or(200) == 200 {
            match operation {
                "login_qr_key" if response.body.get("unikey").is_some() => {
                    response.body = json!({"code":200,"data":response.body});
                }
                "login_qr_create" if query.get_bool("qrimg", false) => {
                    let url = response.body["data"]["qrurl"]
                        .as_str()
                        .filter(|url| !url.is_empty())
                        .ok_or("登录二维码地址为空")?
                        .to_owned();
                    let png = tokio::task::spawn_blocking(move || qr_image(&url))
                        .await
                        .map_err(|_| "生成登录二维码失败".to_owned())??;
                    response.body["data"]["qrimg"] = png.into();
                }
                "playlist_update" => {
                    for path in UPDATE_PATHS {
                        let result = &response.body[path];
                        if result["code"] != 200 {
                            return Ok(ApiResponse {
                                status: response.status,
                                body: json!({
                                    "code": result["code"].as_i64().filter(|code| *code != 0).unwrap_or(502),
                                    "message": result["message"].as_str()
                                        .or_else(|| result["msg"].as_str())
                                        .unwrap_or("歌单信息未完整保存，请刷新后重试")
                                }),
                                cookie: response.cookie,
                            });
                        }
                    }
                    response.body = json!({"code":200});
                }
                _ => {}
            }
        }
        Ok(response)
    }
}

impl MusicApi for NativeApi {
    fn call(&self, operation: &str, params: Value, cookie: &str) -> ApiFuture<'_> {
        let operation = operation.to_owned();
        let query = prepare_query(&operation, params, cookie);
        Box::pin(async move {
            let query = query?;
            #[cfg(test)]
            let query = Query {
                domain: self.domain.clone(),
                ..query
            };
            let _permit = self
                .in_flight
                .try_acquire()
                .map_err(|_| "API 请求过多，请稍后重试".to_owned())?;
            let request_id = native::diagnostics::next_id();
            // Only resolving playback URLs is replayed. Keep writes, login and
            // permission/rate-limit failures single-shot. Both attempts share
            // one timeout and one permit, and cancellation drops the whole future.
            tokio::time::timeout(self.timeout, async {
                for attempt in 1..=2 {
                    let started = Instant::now();
                    let result = self.dispatch(&operation, &query).await;
                    let retry = operation == "song_url" && attempt == 1 && retryable(&result);
                    native::diagnostics::record(
                        if result.as_ref().is_ok_and(|r| r.status == 200 && r.body["code"].as_i64().unwrap_or(200) == 200) { "info" } else { "warn" },
                        "api.attempt_finished",
                        json!({"request_id": request_id, "operation": operation, "song_id": query.get_or("id", ""),
                            "quality": query.get_or("level", ""), "attempt": attempt,
                            "elapsed_ms": started.elapsed().as_millis(), "retry": retry,
                            "sdk_status": result.as_ref().ok().map(|r| r.status),
                            "code": result.as_ref().ok().map(|r| &r.body["code"]),
                            "actual_quality": result.as_ref().ok().map(|r| &r.body["data"][0]["level"]),
                            "error": result.as_ref().err()}),
                    );
                    if !retry { return result; }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                unreachable!("second attempt always returns")
            })
                .await
                .map_err(|_| TIMEOUT_MESSAGE.to_owned())?
        })
    }
}

fn retryable(result: &Result<ApiResponse, String>) -> bool {
    match result {
        Ok(response) => matches!(
            response.body["code"]
                .as_i64()
                .unwrap_or(i64::from(response.status)),
            500 | 502 | 504 | 520..=524
        ),
        Err(error) => error == NETWORK_MESSAGE || error == TIMEOUT_MESSAGE,
    }
}

fn prepare_query(operation: &str, params: Value, cookie: &str) -> Result<Query, String> {
    if params.to_string().len() + cookie.len() > 1024 * 1024 {
        return Err("API 请求过大".into());
    }
    let mut query = Query::new().cookie(cookie);
    match params {
        Value::Null => {}
        Value::Object(params) => {
            for (key, value) in params {
                // Session ownership stays with Services, never with operation params.
                if key == "cookie" || value.is_null() {
                    continue;
                }
                let value = match value {
                    Value::String(value) => value,
                    Value::Array(values) => values
                        .iter()
                        .map(|value| match value {
                            Value::String(value) => value.clone(),
                            _ => value.to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(","),
                    value => value.to_string(),
                };
                query.params.insert(key, value);
            }
        }
        _ => return Err("API 参数必须是对象".into()),
    }
    match operation {
        "playlist_track_add" | "playlist_track_delete" => {
            query = query.param(
                "op",
                if operation == "playlist_track_add" {
                    "add"
                } else {
                    "del"
                },
            );
        }
        "playlist_update" => {
            let id = query.get_or("id", "");
            let name = query.get_or("name", "");
            let desc = query.get_or("desc", "");
            // The SDK's playlist_update also changes tags and interpolates raw JSON.
            // Use batch with only the two fields edited by this application.
            query.params.clear();
            query = query
                .param(UPDATE_PATHS[0], &json!({"id":id,"name":name}).to_string())
                .param(UPDATE_PATHS[1], &json!({"id":id,"desc":desc}).to_string());
        }
        _ => {}
    }
    Ok(query)
}

fn local_response(body: Value) -> ApiResponse {
    ApiResponse {
        status: 200,
        body,
        cookie: vec![],
    }
}

fn rejection(code: i64, message: &str) -> ApiResponse {
    ApiResponse {
        status: u16::try_from(code)
            .ok()
            .filter(|code| (400..600).contains(code))
            .unwrap_or(400),
        body: json!({"code":code,"message":message}),
        cookie: vec![],
    }
}

fn adapt_response(
    result: ncm_api_rs::error::Result<ncm_api_rs::ApiResponse>,
) -> Result<ApiResponse, String> {
    match result {
        Ok(mut response) => {
            // The SDK accepts numeric strings and maps QR/error codes to status 200.
            // Keep those business codes visible to checked() and the QR controller.
            if let Some(code) = response.body["code"]
                .as_str()
                .and_then(|code| code.parse::<i64>().ok())
            {
                response.body["code"] = code.into();
            }
            Ok(ApiResponse {
                status: u16::try_from(response.status).map_err(|_| "API 响应状态无效")?,
                body: response.body,
                cookie: response.cookie,
            })
        }
        Err(NcmError::Api { code, msg }) => Ok(sdk_rejection(code, &msg)),
        Err(NcmError::Response {
            endpoint,
            http_status,
            code,
            message,
            mut body,
            cookie,
        }) => {
            native::diagnostics::record(
                "error",
                "api.server_rejected",
                json!({
                    "endpoint": endpoint, "http_status": http_status, "code": code,
                    "message": message,
                }),
            );
            if !body.is_object() {
                body = json!({});
            }
            body["code"] = code.into();
            if !super::meaningful_message(body["message"].as_str().unwrap_or(""))
                && !super::meaningful_message(body["msg"].as_str().unwrap_or(""))
            {
                body["message"] = super::rejection_message(code).into();
            }
            Ok(ApiResponse {
                status: http_status,
                body,
                cookie,
            })
        }
        Err(NcmError::AuthRequired(message)) => Ok(sdk_rejection(301, &message)),
        Err(NcmError::InvalidParam(message)) => Ok(sdk_rejection(400, &message)),
        Err(NcmError::RateLimited(message)) => Ok(sdk_rejection(503, &message)),
        Err(NcmError::Timeout(_)) => Err(TIMEOUT_MESSAGE.into()),
        Err(NcmError::Http(error)) => {
            let timeout = error.is_timeout();
            let connect = error.is_connect();
            let error = error.without_url();
            native::diagnostics::record(
                "error",
                "api.transport_failed",
                json!({
                    "timeout": timeout, "connect": connect,
                    "causes": native::diagnostics::error_chain(&error),
                }),
            );
            Err(if timeout {
                TIMEOUT_MESSAGE
            } else {
                NETWORK_MESSAGE
            }
            .into())
        }
        Err(NcmError::Crypto(message)) => {
            native::diagnostics::record("error", "api.crypto_failed", json!({"cause": message}));
            Err(format!("网易云协议加解密失败：{message}"))
        }
        Err(NcmError::Json(error)) => Err(format!("网易云响应 JSON 解析失败：{error}")),
        // Unknown SDK errors may contain credentials; expose the failing component.
        Err(NcmError::Unknown(_)) => Err("网易云 SDK 请求处理失败，未提供可公开的错误说明".into()),
    }
}

fn sdk_rejection(code: i64, message: &str) -> ApiResponse {
    rejection(
        code,
        &if super::meaningful_message(message) {
            message.to_owned()
        } else {
            super::rejection_message(code)
        },
    )
}

fn qr_image(url: &str) -> Result<String, String> {
    let code = qrcode::QrCode::new(url.as_bytes()).map_err(|_| "生成登录二维码失败")?;
    let image = image::DynamicImage::ImageLuma8(
        code.render::<image::Luma<u8>>()
            .min_dimensions(256, 256)
            .build(),
    );
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "编码登录二维码失败")?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
    ))
}

#[cfg(test)]
mod tests;
