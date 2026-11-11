//! Native Rust and optional HTTP providers share the same application contract.
//! HTTP providers implement the same version 1 envelope at NCM_API_URL.
mod native;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

pub type ApiFuture<'a> = Pin<Box<dyn Future<Output = Result<ApiResponse, String>> + Send + 'a>>;
pub trait MusicApi: Send + Sync {
    fn call(&self, operation: &str, params: Value, cookie: &str) -> ApiFuture<'_>;
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub status: u16,
    pub body: Value,
    #[serde(default)]
    pub cookie: Vec<String>,
}
impl ApiResponse {
    pub fn checked(self) -> Result<Self, String> {
        if !self.body.is_object() {
            return Err("API 未返回有效数据".into());
        }
        let code = self.body["code"].as_i64().unwrap_or(200);
        if !(200..300).contains(&self.status) || code != 200 {
            return Err(self.body["message"]
                .as_str()
                .or(self.body["msg"].as_str())
                .unwrap_or("网易云服务拒绝了请求，请检查登录状态和权限")
                .to_owned());
        }
        Ok(self)
    }
    pub fn session_cookie(&self) -> String {
        if let Some(cookie) = self.body["cookie"].as_str().filter(|v| !v.is_empty()) {
            return cookie.to_owned();
        }
        self.cookie
            .iter()
            .filter_map(|v| v.split(';').next())
            .collect::<Vec<_>>()
            .join("; ")
    }
}
struct HttpApi {
    endpoint: String,
    client: reqwest::Client,
}
impl MusicApi for HttpApi {
    fn call(&self, operation: &str, params: Value, cookie: &str) -> ApiFuture<'_> {
        let body =
            serde_json::json!({"version":1,"operation":operation,"params":params,"cookie":cookie});
        Box::pin(async move {
            let response = self
                .client
                .post(&self.endpoint)
                .json(&body)
                .send()
                .await
                .map_err(|_| "无法连接 API 服务；未自动重试请求".to_owned())?;
            if !response.status().is_success() {
                return Err(format!("API 服务返回 HTTP {}", response.status().as_u16()));
            }
            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| "API 响应中断".to_owned())?;
                if bytes.len() + chunk.len() > 16 * 1024 * 1024 {
                    return Err("API 响应过大".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            let envelope: Value =
                serde_json::from_slice(&bytes).map_err(|_| "API 响应格式错误".to_owned())?;
            if envelope["version"] != 1 {
                return Err("API 协议版本不受支持".into());
            }
            serde_json::from_value(envelope).map_err(|_| "API 响应格式错误".into())
        })
    }
}
pub fn configured_provider() -> Result<Arc<dyn MusicApi>, String> {
    if let Ok(endpoint) = std::env::var("NCM_API_URL") {
        let url = reqwest::Url::parse(&endpoint).map_err(|_| "NCM_API_URL 无效".to_owned())?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if url.scheme() != "https" && !(url.scheme() == "http" && local) {
            return Err("远程 API 必须使用 HTTPS；本地开发允许 HTTP".into());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(40))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(HttpApi { endpoint, client }))
    } else {
        Ok(Arc::new(native::NativeApi::new()))
    }
}
