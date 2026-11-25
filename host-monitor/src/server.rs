use crate::{collectors::Collector, docker};
use http_body_util::Full;
use hyper::{
    Request, Response, StatusCode,
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use std::{
    convert::Infallible,
    io,
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::UnixListener,
    sync::{Mutex, Semaphore},
};
use yaya_ops_protocol::{
    Capability, HostMonitorHello, HostMonitorReply, MetricsSnapshot, PROTOCOL_VERSION,
};

pub struct HostMonitorState {
    pub node_id: String,
    pub name: String,
    pub boot_id: String,
    pub docker_socket: PathBuf,
    pub allowed_uid: u32,
    collector: Arc<Collector>,
    snapshot: Mutex<Option<(Instant, MetricsSnapshot)>>,
}

impl HostMonitorState {
    pub fn new(node_id: String, docker_socket: PathBuf, allowed_uid: u32) -> Self {
        Self {
            node_id,
            name: std::fs::read_to_string("/proc/sys/kernel/hostname")
                .unwrap_or_else(|_| "localhost".into())
                .trim()
                .chars()
                .take(128)
                .collect(),
            boot_id: std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
                .trim()
                .into(),
            docker_socket,
            allowed_uid,
            collector: Arc::new(Collector::default()),
            snapshot: Mutex::new(None),
        }
    }

    async fn snapshot(&self) -> io::Result<MetricsSnapshot> {
        let mut cached = self.snapshot.lock().await;
        if let Some((when, value)) = cached.as_ref() {
            if when.elapsed() < Duration::from_secs(5) {
                return Ok(value.clone());
            }
        }
        let collector = self.collector.clone();
        let node = self.node_id.clone();
        let boot = self.boot_id.clone();
        let snapshot = tokio::task::spawn_blocking(move || collector.sample(&node, &boot))
            .await
            .map_err(io::Error::other)?;
        *cached = Some((Instant::now(), snapshot.clone()));
        Ok(snapshot)
    }
}

fn json<T: serde::Serialize>(
    status: StatusCode,
    value: &HostMonitorReply<T>,
) -> Response<Full<Bytes>> {
    let bytes = serde_json::to_vec(value).expect("protocol values serialize");
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("cache-control", "no-store")
        .body(Full::new(Bytes::from(bytes)))
        .unwrap()
}

async fn route(
    request: Request<Incoming>,
    state: Arc<HostMonitorState>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    if request.method() != hyper::Method::GET {
        return Ok(json(
            StatusCode::METHOD_NOT_ALLOWED,
            &HostMonitorReply::<()>::failure(
                "READ_ONLY",
                "This host monitor only exposes read-only endpoints",
            ),
        ));
    }
    let response = match request.uri().path() {
        "/v1/hello" => {
            let docker = match docker::api_prefix(&state.docker_socket).await {
                Ok(_) => Capability::available(),
                Err(_) => Capability::unavailable("docker_unavailable_or_unsupported"),
            };
            match state.snapshot().await {
                Ok(snapshot) => json(
                    StatusCode::OK,
                    &HostMonitorReply::success(HostMonitorHello {
                        node_id: state.node_id.clone(),
                        name: state.name.clone(),
                        boot_id: state.boot_id.clone(),
                        monitor_version: env!("CARGO_PKG_VERSION").into(),
                        protocol_version: PROTOCOL_VERSION,
                        docker,
                        host_metrics: snapshot.host_capability,
                        nvidia: snapshot.gpu_capability,
                    }),
                ),
                Err(_) => json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &HostMonitorReply::<()>::failure(
                        "METRICS_UNAVAILABLE",
                        "Snapshot could not be collected",
                    ),
                ),
            }
        }
        "/v1/snapshot" => match state.snapshot().await {
            Ok(snapshot) => json(StatusCode::OK, &HostMonitorReply::success(snapshot)),
            Err(_) => json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &HostMonitorReply::<()>::failure(
                    "METRICS_UNAVAILABLE",
                    "Snapshot could not be collected",
                ),
            ),
        },
        // Domain errors use an envelope so the bounded local client can preserve the code.
        "/v1/containers" => match docker::list(&state.docker_socket).await {
            Ok(containers) => json(StatusCode::OK, &HostMonitorReply::success(containers)),
            Err(_) => json(
                StatusCode::OK,
                &HostMonitorReply::<()>::failure(
                    "DOCKER_UNAVAILABLE",
                    "Docker is unavailable or its API is unsupported",
                ),
            ),
        },
        _ => json(
            StatusCode::NOT_FOUND,
            &HostMonitorReply::<()>::failure("NOT_FOUND", "Unknown host monitor endpoint"),
        ),
    };
    Ok(response)
}

pub async fn serve(listener: UnixListener, state: Arc<HostMonitorState>) -> io::Result<()> {
    let permits = Arc::new(Semaphore::new(16));
    loop {
        let (stream, _) = listener.accept().await?;
        let credential = stream.peer_cred()?;
        if credential.uid() != 0 && credential.uid() != state.allowed_uid {
            continue;
        }
        let permit = match permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => continue,
        };
        let state = state.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let service = service_fn(move |request| route(request, state.clone()));
            let _ = tokio::time::timeout(
                Duration::from_secs(10),
                http1::Builder::new()
                    .keep_alive(false)
                    .max_buf_size(16 * 1024)
                    .serve_connection(TokioIo::new(stream), service),
            )
            .await;
        });
    }
}

fn identity(directory: &Path) -> io::Result<String> {
    std::fs::create_dir_all(directory)?;
    if std::fs::symlink_metadata(directory)?
        .file_type()
        .is_symlink()
    {
        return Err(io::Error::other(
            "Host monitor data directory must not be a symlink",
        ));
    }
    let path = directory.join("identity.json");
    let new_id = format!("node_{}", uuid::Uuid::new_v4().simple());
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(serde_json::to_string(&new_id)?.as_bytes())?;
            file.sync_all()?;
            Ok(new_id)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(&path)?;
            if !metadata.is_file() || metadata.len() > 256 {
                return Err(io::Error::other("Invalid host monitor identity file"));
            }
            let id: String = serde_json::from_slice(&std::fs::read(&path)?)?;
            let suffix = id
                .strip_prefix("node_")
                .ok_or_else(|| io::Error::other("Invalid node ID"))?;
            uuid::Uuid::parse_str(suffix).map_err(io::Error::other)?;
            Ok(id)
        }
        Err(error) => Err(error),
    }
}

struct SocketGuard {
    path: PathBuf,
    inode: u64,
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.file_type().is_socket() && m.ino() == self.inode)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub async fn run() -> io::Result<()> {
    let socket = PathBuf::from(
        std::env::var_os("YAYA_HOST_MONITOR_SOCKET")
            .or_else(|| std::env::var_os("YAYA_OPS_AGENT_SOCKET"))
            .unwrap_or_else(|| "/run/yaya-host-monitor/monitor.sock".into()),
    );
    let directory = PathBuf::from(
        std::env::var_os("YAYA_HOST_MONITOR_DATA_DIR")
            .or_else(|| std::env::var_os("YAYA_OPS_AGENT_DATA_DIR"))
            .unwrap_or_else(|| {
                // Preserve the node identity of an existing installation.
                if Path::new("/var/lib/yaya-ops-agent/identity.json").exists() {
                    "/var/lib/yaya-ops-agent".into()
                } else {
                    "/var/lib/yaya-host-monitor".into()
                }
            }),
    );
    let docker = PathBuf::from(
        std::env::var_os("YAYA_OPS_DOCKER_SOCKET").unwrap_or_else(|| "/var/run/docker.sock".into()),
    );
    let allowed_uid = std::env::var("YAYA_HOST_MONITOR_ALLOWED_UID")
        .or_else(|_| std::env::var("YAYA_OPS_AGENT_ALLOWED_UID"))
        .unwrap_or_else(|_| "10001".into())
        .parse::<u32>()
        .map_err(io::Error::other)?;
    let node_id = identity(&directory)?;
    let parent = socket
        .parent()
        .ok_or_else(|| io::Error::other("Socket requires a parent directory"))?;
    std::fs::create_dir_all(parent)?;
    if std::fs::symlink_metadata(parent)?.file_type().is_symlink() {
        return Err(io::Error::other("Socket directory must not be a symlink"));
    }
    match std::fs::symlink_metadata(&socket) {
        Ok(metadata) => {
            // Only remove a stale socket owned by this service identity.
            let own_uid = unsafe { libc::geteuid() };
            if !metadata.file_type().is_socket() || metadata.uid() != own_uid {
                return Err(io::Error::other("Refusing to replace existing socket path"));
            }
            if tokio::net::UnixStream::connect(&socket).await.is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "Host monitor already running",
                ));
            }
            std::fs::remove_file(&socket)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o660))?;
    let _guard = SocketGuard {
        path: socket.clone(),
        inode: std::fs::metadata(&socket)?.ino(),
    };
    eprintln!(
        "Yaya read-only host monitor {} listening on {} ({node_id})",
        env!("CARGO_PKG_VERSION"),
        socket.display()
    );
    let state = Arc::new(HostMonitorState::new(node_id, docker, allowed_uid));
    tokio::select! { result=serve(listener,state)=>result, result=tokio::signal::ctrl_c()=>result }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_survives_restart_and_invalid_files_are_not_replaced() {
        let directory =
            std::env::temp_dir().join(format!("yaya-host-monitor-{}", uuid::Uuid::new_v4()));
        let first = identity(&directory).unwrap();
        assert_eq!(identity(&directory).unwrap(), first);
        std::fs::write(directory.join("identity.json"), b"invalid").unwrap();
        assert!(identity(&directory).is_err());
        std::fs::remove_file(directory.join("identity.json")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[tokio::test]
    async fn real_uds_transport_reports_docker_absence_and_rejects_unknown_routes() {
        let socket =
            std::env::temp_dir().join(format!("yaya-host-monitor-{}.sock", uuid::Uuid::new_v4()));
        let listener = UnixListener::bind(&socket).unwrap();
        let state = Arc::new(HostMonitorState::new(
            "node_test".into(),
            PathBuf::from("/nonexistent/yaya-docker.sock"),
            unsafe { libc::geteuid() },
        ));
        let server = tokio::spawn(serve(listener, state));
        let hello: HostMonitorReply<HostMonitorHello> =
            yaya_ops_protocol::local::get_json(&socket, "/v1/hello")
                .await
                .unwrap();
        assert!(!hello.data.unwrap().docker.available);
        let containers: HostMonitorReply<yaya_ops_protocol::ContainerList> =
            yaya_ops_protocol::local::get_json(&socket, "/v1/containers")
                .await
                .unwrap();
        assert_eq!(containers.error.unwrap().code, "DOCKER_UNAVAILABLE");
        assert!(
            yaya_ops_protocol::local::get_json::<serde_json::Value>(&socket, "/v1/executions")
                .await
                .is_err()
        );
        server.abort();
        std::fs::remove_file(socket).unwrap();
    }
}
