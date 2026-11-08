//! A provider implements this contract; neither the UI nor the native player knows Node.
//! HTTP providers implement the same version 1 envelope at NCM_API_URL.
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap, future::Future, path::PathBuf, pin::Pin, process::Stdio, sync::Arc,
    time::Duration,
};
use tokio::{
    process::Command,
    sync::{mpsc, oneshot},
};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

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
type Pending = (Value, oneshot::Sender<Result<ApiResponse, String>>);
struct NodeApi {
    sender: mpsc::Sender<Pending>,
}
impl NodeApi {
    fn start() -> Self {
        let packaged = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("sidecar/music-api.cjs")));
        let script = std::env::var_os("NCM_API_SIDECAR")
            .map(PathBuf::from)
            .or_else(|| packaged.filter(|p| p.is_file()))
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sidecar/music-api.cjs")
            });
        Self::launch(script, Duration::from_secs(40))
    }
    fn launch(script: PathBuf, timeout: Duration) -> Self {
        let (sender, mut requests) = mpsc::channel::<Pending>(32);
        tokio::spawn(async move {
            let result =
                Command::new(std::env::var_os("NCM_NODE").unwrap_or_else(|| "node".into()))
                    .arg(script)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn();
            let Ok(mut child) = result else {
                while let Some((_, reply)) = requests.recv().await {
                    let _ = reply.send(Err(
                        "无法启动网易云 API，请安装 Node.js 和 npm 依赖，或设置 NCM_API_URL".into(),
                    ));
                }
                return;
            };
            let mut input = FramedWrite::new(
                child.stdin.take().unwrap(),
                LinesCodec::new_with_max_length(1024 * 1024),
            );
            let mut output = FramedRead::new(
                child.stdout.take().unwrap(),
                LinesCodec::new_with_max_length(16 * 1024 * 1024),
            );
            let mut pending: HashMap<
                u64,
                (
                    tokio::time::Instant,
                    oneshot::Sender<Result<ApiResponse, String>>,
                ),
            > = HashMap::new();
            let mut id = 0u64;
            let mut tick = tokio::time::interval(timeout.min(Duration::from_secs(1)));
            loop {
                tokio::select! {
                    request = requests.recv() => {
                        let Some((mut body, reply)) = request else { break };
                        if pending.len() >= 64 { let _ = reply.send(Err("API 请求过多，请稍后重试".into())); continue; }
                        id += 1; body["id"] = id.into();
                        pending.insert(id, (tokio::time::Instant::now(), reply));
                        let frame=body.to_string();
                        if frame.len()>1024*1024 { if let Some((_,reply))=pending.remove(&id){let _=reply.send(Err("API 请求过大".into()));}continue; }
                        if !matches!(tokio::time::timeout(timeout,input.send(frame)).await,Ok(Ok(()))) { break }
                    }
                    line = output.next() => {
                        let Some(Ok(line)) = line else { break };
                        let Ok(value) = serde_json::from_str::<Value>(&line) else { break };
                        if value["version"] != 1 { break }
                        if let Some(id) = value["id"].as_u64() {
                            if let Some((_, reply)) = pending.remove(&id) {
                                let _ = reply.send(serde_json::from_value(value).map_err(|_| "API 响应格式错误".into()));
                            }
                        }
                    }
                    _ = tick.tick() => {
                        let expired: Vec<_> = pending.iter().filter(|(_, (t, _))| t.elapsed() > timeout).map(|(id,_)| *id).collect();
                        for id in expired { if let Some((_, reply)) = pending.remove(&id) { let _ = reply.send(Err("API 请求超时；操作可能已执行，请刷新确认".into())); } }
                    }
                }
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            for (_, (_, reply)) in pending {
                let _ = reply.send(Err(
                    "网易云 API 进程已退出，请检查依赖后重启；未自动重试操作".into(),
                ));
            }
            // Closing the receiver causes all subsequent requests to fail promptly.
        });
        Self { sender }
    }
}
impl MusicApi for NodeApi {
    fn call(&self, operation: &str, params: Value, cookie: &str) -> ApiFuture<'_> {
        let request =
            serde_json::json!({"version":1,"operation":operation,"params":params,"cookie":cookie});
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.sender
                .send((request, tx))
                .await
                .map_err(|_| "网易云 API 未运行，请重启应用".to_owned())?;
            rx.await.map_err(|_| "网易云 API 连接已关闭".to_owned())?
        })
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
        Ok(Arc::new(NodeApi::start()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn node_transport_correlates_out_of_order_responses_and_does_not_retry_timeouts() {
        let dir = std::env::temp_dir().join(format!("ncm-api-transport-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("mock.cjs");
        std::fs::write(
            &script,
            r#"
const readline = require('node:readline');
let count = 0;
readline.createInterface({input:process.stdin}).on('line', line => {
  const r=JSON.parse(line); count++;
  if(r.operation==='exit') process.exit(0);
  if(r.operation==='timeout') return;
  const body={version:1,id:r.id,status:200,body:{code:200,operation:r.operation, count},cookie:[]};
  setTimeout(()=>process.stdout.write(JSON.stringify(body)+'\n'),r.operation==='slow'?30:0);
});
"#,
        )
        .unwrap();
        let api = NodeApi::launch(script, Duration::from_millis(150));
        let (a, b) = tokio::join!(
            api.call("slow", Value::Null, ""),
            api.call("fast", Value::Null, "")
        );
        assert_eq!(a.unwrap().body["operation"], "slow");
        assert_eq!(b.unwrap().body["operation"], "fast");
        assert!(api
            .call("timeout", Value::Null, "")
            .await
            .unwrap_err()
            .contains("超时"));
        assert_eq!(
            api.call("after-timeout", Value::Null, "")
                .await
                .unwrap()
                .body["count"],
            4
        );
        assert!(api.call("exit", Value::Null, "").await.is_err());
        assert!(api.call("after-exit", Value::Null, "").await.is_err());
        drop(api);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
