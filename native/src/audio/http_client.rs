use stream_download::http::{Client as StreamClient, RANGE_HEADER_KEY, format_range_header_bytes};

/// 包装 reqwest::Client，修正 stream-download 可能产生的倒置 range 请求。
///
/// stream-download 0.24 的 `handle_seek` 在 downloaded 集合存在多个 gap 时
/// （例如先回退 seek 留下靠前的 gap，再向前 seek 到未缓存位置），只夹紧了
/// `seek_start = gap.start.max(position)` 而没有校验 `gap.end`，可能发出
/// `start > end` 的倒置 range 请求；服务器返回 416 后整个下载任务被标记为
/// 失败（"stream failed to download"），播放随之中断。
///
/// 倒置的 end 直接丢弃即可：下载任务发起 range 请求时的真实意图就是从
/// start 位置继续下载到流末尾（writer 也已 seek 到 start）。
#[derive(Clone)]
pub(crate) struct RangeSanitizingClient(reqwest::Client);

impl RangeSanitizingClient {
    pub(crate) fn new(client: reqwest::Client) -> Self {
        Self(client)
    }
}

impl StreamClient for RangeSanitizingClient {
    type Url = reqwest::Url;
    type Headers = reqwest::header::HeaderMap;
    type Response = reqwest::Response;
    type Error = reqwest::Error;

    fn create() -> Self {
        Self(reqwest::Client::new())
    }

    async fn get(&self, url: &Self::Url) -> Result<Self::Response, Self::Error> {
        self.0.get(url.clone()).send().await
    }

    async fn get_range(
        &self,
        url: &Self::Url,
        start: u64,
        end: Option<u64>,
    ) -> Result<Self::Response, Self::Error> {
        // stream-download 在多 gap 场景可能算出 start > end；丢弃非法 end，
        // 按 open-ended range 从 start 继续下载（写端也已 seek 到 start）。
        let end = end.filter(|end| *end >= start);
        self.0
            .get(url.clone())
            .header(RANGE_HEADER_KEY, format_range_header_bytes(start, end))
            .send()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use stream_download::http::Client;

    #[tokio::test]
    async fn inverted_range_is_sanitized_to_open_ended_request() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept test request");
            let mut request = [0u8; 4096];
            let read = socket.read(&mut request).expect("read request");
            let request_text = String::from_utf8_lossy(&request[..read]);
            let range_line = request_text
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                .map(str::to_string);
            let body = range_line.unwrap_or_default();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let client = RangeSanitizingClient::new(reqwest::Client::new());
        let url: reqwest::Url = format!("http://{address}/song").parse().expect("parse url");
        let response = Client::get_range(&client, &url, 2_500_000, Some(2_000_000))
            .await
            .expect("send sanitized range request");
        let body = response.text().await.expect("read response body");
        server.join().expect("join test server");

        assert!(
            body.eq_ignore_ascii_case("Range: bytes=2500000-"),
            "sanitized request must use an open-ended range, got: {body}"
        );
    }
}
