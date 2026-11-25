//! Bounded HTTP/1 GET over a local Unix socket. No network fallback on unsupported hosts.
use serde::de::DeserializeOwned;
use std::{io, path::Path};

#[cfg(unix)]
struct ConnectionTask(tokio::task::JoinHandle<()>);
#[cfg(unix)]
impl Drop for ConnectionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(unix)]
pub async fn get_json<T: DeserializeOwned>(socket: &Path, path: &str) -> io::Result<T> {
    use http_body_util::{BodyExt, Empty, Limited};
    use hyper::{Request, body::Bytes, client::conn::http1};
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;

    let operation = async {
        let stream = UnixStream::connect(socket).await?;
        let (mut sender, connection) = http1::handshake(TokioIo::new(stream))
            .await
            .map_err(io::Error::other)?;
        let _driver = ConnectionTask(tokio::spawn(async move {
            let _ = connection.await;
        }));
        let result = async {
            let request = Request::get(path)
                .header("host", "localhost")
                .header("connection", "close")
                .body(Empty::<Bytes>::new())
                .map_err(io::Error::other)?;
            let response = sender
                .send_request(request)
                .await
                .map_err(io::Error::other)?;
            if !response.status().is_success() {
                return Err(io::Error::other(format!(
                    "local service HTTP {}",
                    response.status()
                )));
            }
            let bytes = Limited::new(response.into_body(), super::MAX_RESPONSE_BYTES)
                .collect()
                .await
                .map_err(io::Error::other)?
                .to_bytes();
            serde_json::from_slice(&bytes).map_err(io::Error::other)
        }
        .await;
        result
    };
    tokio::time::timeout(std::time::Duration::from_secs(3), operation)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "local service timed out"))?
}

#[cfg(not(unix))]
pub async fn get_json<T: DeserializeOwned>(_socket: &Path, _path: &str) -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Unix socket requires Linux/Unix",
    ))
}
