//! Fixed local Engine transport: Windows named pipe or Linux Unix socket; no remote endpoint input.
use http_body_util::{BodyExt, Full, Limited};
use hyper::{
    Request,
    body::{Bytes, Incoming},
    client::conn::http1,
};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use std::{pin::Pin, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Transport for T {}
pub type Socket = Pin<Box<dyn Transport>>;

async fn connect() -> Result<Socket, String> {
    #[cfg(windows)]
    {
        Ok(Box::pin(
            tokio::net::windows::named_pipe::ClientOptions::new()
                .open(r"\\.\pipe\docker_engine")
                .map_err(|e| format!("Docker Desktop 无法连接：{e}"))?,
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(Box::pin(
            tokio::net::UnixStream::connect("/var/run/docker.sock")
                .await
                .map_err(|e| format!("本机 Docker 无法连接：{e}"))?,
        ))
    }
}

async fn raw_request(
    method: &str,
    path: &str,
    body: Option<Value>,
    upgrade: bool,
) -> Result<hyper::Response<Incoming>, String> {
    raw_request_on(connect().await?, method, path, body, upgrade).await
}
async fn raw_request_on(
    socket: Socket,
    method: &str,
    path: &str,
    body: Option<Value>,
    upgrade: bool,
) -> Result<hyper::Response<Incoming>, String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (mut sender, conn) = http1::handshake(TokioIo::new(socket))
            .await
            .map_err(|e| e.to_string())?;
        tokio::spawn(async move {
            let _ = conn.with_upgrades().await;
        });
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("Host", "localhost")
            .header("Content-Type", "application/json");
        if upgrade {
            req = req.header("Connection", "Upgrade").header("Upgrade", "tcp");
        }
        let req = req
            .body(Full::new(Bytes::from(
                body.map(|v| v.to_string()).unwrap_or_default(),
            )))
            .map_err(|e| e.to_string())?;
        let response = sender
            .send_request(req)
            .await
            .map_err(|e| format!("Docker 请求失败：{e}"))?;
        if !response.status().is_success() && response.status() != 101 {
            let status = response.status();
            let bytes = Limited::new(response.into_body(), 65536)
                .collect()
                .await
                .map_err(|e| e.to_string())?
                .to_bytes();
            let detail = crate::platform_delivery::docker_diagnostic(&bytes, &[]);
            return Err(format!("Docker HTTP {status}：{detail}"));
        }
        Ok(response)
    })
    .await
    .map_err(|_| "Docker 响应超时".to_string())?
}
// Short-lived daemon capability cache; failed probes are never cached.
static API_VERSION: tokio::sync::Mutex<Option<(std::time::Instant, String)>> =
    tokio::sync::Mutex::const_new(None);
fn version_pair(text: &str) -> Result<(u16, u16), String> {
    let (major, minor) = text.split_once('.').ok_or("Docker API 版本格式无效")?;
    Ok((
        major.parse().map_err(|_| "Docker API 版本格式无效")?,
        minor.parse().map_err(|_| "Docker API 版本格式无效")?,
    ))
}
fn select_version(value: &Value) -> Result<String, String> {
    let maximum = version_pair(
        value["ApiVersion"]
            .as_str()
            .ok_or("Docker 未返回 API 版本")?,
    )?;
    let minimum = version_pair(value["MinAPIVersion"].as_str().unwrap_or("1.24"))?;
    let selected = maximum.min((1, 47));
    if selected < minimum || selected < (1, 24) {
        return Err("Docker API 版本范围不受支持，请检查 Docker Engine 版本".into());
    }
    Ok(format!("{}.{}", selected.0, selected.1))
}
async fn api_version() -> Result<String, String> {
    let mut cached = API_VERSION.lock().await;
    if let Some((time, version)) = cached.as_ref() {
        if time.elapsed() < Duration::from_secs(30) {
            return Ok(version.clone());
        }
    }
    // Probe without a version prefix so older daemons can answer.
    let value = response_json(raw_request("GET", "/version", None, false).await?).await?;
    let version = select_version(&value)?;
    *cached = Some((std::time::Instant::now(), version.clone()));
    Ok(version)
}
pub async fn request(
    method: &str,
    path: &str,
    body: Option<Value>,
    upgrade: bool,
) -> Result<hyper::Response<Incoming>, String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let version = api_version().await?;
        raw_request(method, &format!("/v{version}{path}"), body, upgrade).await
    })
    .await
    .map_err(|_| "Docker 响应超时".to_string())?
}
pub async fn json(method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    response_json(request(method, path, body, false).await?).await
}
async fn response_json(response: hyper::Response<Incoming>) -> Result<Value, String> {
    let bytes = tokio::time::timeout(
        Duration::from_secs(10),
        Limited::new(response.into_body(), 4 * 1024 * 1024).collect(),
    )
    .await
    .map_err(|_| "Docker 数据读取超时")?
    .map_err(|e| e.to_string())?
    .to_bytes();
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Docker 返回无效 JSON".into())
}

/// Incremental Docker stdout/stderr multiplex decoder. TTY output is a single combined stream.
pub struct Decoder {
    pub tty: bool,
    buffer: Vec<u8>,
}
impl Decoder {
    pub fn new(tty: bool) -> Self {
        Self {
            tty,
            buffer: Vec::new(),
        }
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
        if self.tty {
            return Ok(vec![("combined".into(), bytes.to_vec())]);
        }
        self.buffer.extend_from_slice(bytes);
        let mut result = vec![];
        while self.buffer.len() >= 8 {
            let n = u32::from_be_bytes(self.buffer[4..8].try_into().unwrap()) as usize;
            if n > 1024 * 1024
                || ![0, 1, 2].contains(&self.buffer[0])
                || self.buffer[1..4] != [0, 0, 0]
            {
                return Err("Docker 日志帧无效或超过 1 MiB".into());
            }
            if self.buffer.len() < n + 8 {
                break;
            }
            let stream = if self.buffer[0] == 2 {
                "stderr"
            } else {
                "stdout"
            };
            result.push((stream.into(), self.buffer[8..8 + n].to_vec()));
            self.buffer.drain(..8 + n);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // A daemon restricted to 1.45 must accept both the log request and TCP exec upgrade.
    #[tokio::test]
    async fn older_daemon_logs_and_exec_use_negotiated_path() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        async fn fake_request(
            path: &str,
            upgrade: bool,
            expected: &str,
            response: &'static [u8],
        ) -> hyper::Response<Incoming> {
            let (client, mut server) = tokio::io::duplex(8192);
            let expected = expected.to_string();
            let task = tokio::spawn(async move {
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    server.read_exact(&mut byte).await.unwrap();
                    request.push(byte[0]);
                }
                let headers = String::from_utf8(request).unwrap();
                assert!(
                    headers.lines().next().unwrap().contains(&expected),
                    "{headers}"
                );
                if upgrade {
                    assert!(headers.to_ascii_lowercase().contains("upgrade: tcp"));
                }
                server.write_all(response).await.unwrap();
            });
            let result = raw_request_on(Box::pin(client), "GET", path, None, upgrade)
                .await
                .unwrap();
            task.await.unwrap();
            result
        }
        let probe = fake_request(
            "/version",
            false,
            "GET /version HTTP/1.1",
            b"HTTP/1.1 200 OK\r\nContent-Length: 21\r\n\r\n{\"ApiVersion\":\"1.45\"}",
        )
        .await;
        // Real responses also include a minimum; legacy responses can omit it.
        let version = select_version(&response_json(probe).await.unwrap()).unwrap();
        let logs = fake_request(
            &format!("/v{version}/containers/test/logs?stdout=1"),
            false,
            "GET /v1.45/containers/test/logs?stdout=1 HTTP/1.1",
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok",
        )
        .await;
        assert_eq!(logs.status(), 200);
        let exec = fake_request(
            &format!("/v{version}/exec/test/start"),
            true,
            "GET /v1.45/exec/test/start HTTP/1.1",
            b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: tcp\r\n\r\n",
        )
        .await;
        assert_eq!(exec.status(), 101);
    }
    #[test]
    fn negotiates_daemon_api_range_and_rejects_invalid_or_unsupported_versions() {
        use serde_json::json;
        assert_eq!(
            select_version(&json!({"ApiVersion":"1.45","MinAPIVersion":"1.24"})).unwrap(),
            "1.45"
        );
        assert_eq!(
            select_version(&json!({"ApiVersion":"1.52","MinAPIVersion":"1.44"})).unwrap(),
            "1.47"
        );
        assert_eq!(select_version(&json!({"ApiVersion":"1.9"})).is_err(), true);
        for value in [
            json!({}),
            json!({"ApiVersion":"broken"}),
            json!({"ApiVersion":"1.52","MinAPIVersion":"1.48"}),
            json!({"ApiVersion":"1.45","MinAPIVersion":"1.46"}),
        ] {
            assert!(select_version(&value).is_err());
        }
    }
    #[test]
    fn fragmented_multiplex_and_tty_are_distinct() {
        let mut d = Decoder::new(false);
        assert!(d.push(&[2, 0, 0, 0, 0]).unwrap().is_empty());
        assert_eq!(
            d.push(&[0, 0, 3, b'e', b'r', b'r']).unwrap(),
            vec![("stderr".into(), b"err".to_vec())]
        );
        assert!(d.push(&[1, 0, 0, 0, 255, 255, 255, 255]).is_err());
        assert_eq!(Decoder::new(true).push(b"hello").unwrap()[0].0, "combined");
    }
}
