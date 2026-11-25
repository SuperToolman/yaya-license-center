//! Local file RPC: the public API keeps authentication and database audit; a separate
//! process owns filesystem permissions. No TCP listener or command execution exists.
use super::*;
use axum::http::{Method, header};
use serde_json::Value;
#[cfg(unix)]
use serde_json::json;

#[cfg(unix)]
pub const PROTOCOL: &str = "1";
#[cfg(unix)]
const PROTOCOL_HEADER: &str = "x-yaya-file-protocol";

fn socket_path() -> Option<PathBuf> {
    std::env::var_os("YAYA_FILE_AGENT_SOCKET")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn operation(method: &Method, path: &str) -> Option<&'static str> {
    match (method.as_str(), path) {
        ("GET", "/api/ops/local/files") => Some("list"),
        ("GET", "/api/ops/local/files/preview") => Some("preview"),
        ("GET", "/api/ops/local/files/properties") => Some("properties"),
        ("GET", "/api/ops/local/files/size") => Some("size"),
        ("GET", "/api/ops/local/files/download") => Some("download"),
        ("POST", "/api/ops/local/files/upload") => Some("upload"),
        ("POST", "/api/ops/local/files/mutate") => Some("mutate"),
        _ => None,
    }
}

pub(super) async fn forward(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let Some(socket) = socket_path() else {
        return next.run(request).await;
    };
    let Some(mut action) = operation(request.method(), request.uri().path()).map(str::to_owned)
    else {
        return failure(
            StatusCode::NOT_FOUND,
            "FILES_OPERATION_DENIED",
            "不支持此文件操作",
            &context.request_id,
        );
    };
    // All public authorization happens before the IPC boundary. Never forward browser
    // cookies, authorization headers or caller-supplied identity headers to the agent.
    if request.method() != Method::GET
        && (!request.headers().contains_key(header::ORIGIN)
            || crate::platform_versions::check_origin(request.headers()).is_err())
    {
        return failure(
            StatusCode::FORBIDDEN,
            "ORIGIN_DENIED",
            "操作来源不允许，请从平台页面操作",
            &context.request_id,
        );
    }
    let mut target = reqwest::Url::parse(&format!("http://files.local{}", request.uri()))
        .ok()
        .and_then(|u| {
            u.query_pairs()
                .find(|(key, _)| key == "path")
                .map(|(_, value)| value.into_owned())
        })
        .unwrap_or_else(|| "/".into());
    if action == "mutate" {
        let (parts, body) = request.into_parts();
        let bytes = match axum::body::to_bytes(body, 32 * 1024).await {
            Ok(bytes) => bytes,
            Err(_) => {
                return failure(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "FILES_BODY_LIMIT",
                    "文件操作参数不得超过 32 KiB",
                    &context.request_id,
                );
            }
        };
        let input: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                return failure(
                    StatusCode::BAD_REQUEST,
                    "FILES_INVALID_BODY",
                    "文件操作参数无效",
                    &context.request_id,
                );
            }
        };
        let Some(name) = input["action"]
            .as_str()
            .filter(|a| ["create", "mkdir", "delete", "rename"].contains(a))
        else {
            return failure(
                StatusCode::BAD_REQUEST,
                "FILES_OPERATION_DENIED",
                "不支持此文件操作",
                &context.request_id,
            );
        };
        action = name.into();
        target = input["path"].as_str().unwrap_or("/").into();
        if let Some(name) = input["name"].as_str() {
            target = format!("{target} → {name}");
        }
        request = Request::from_parts(parts, Body::from(bytes));
    }
    let audit = files::FileAudit::Api(state);
    if files::audit(
        &audit,
        &context,
        &format!("files.agent.{action}.requested"),
        &target,
    )
    .await
    .is_err()
    {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "FILES_AUDIT_UNAVAILABLE",
            "审计记录不可用，文件操作未执行",
            &context.request_id,
        );
    }
    let response = match exchange(&socket, &context, request).await {
        Ok(response) => response,
        Err(message) => {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "FILE_AGENT_UNAVAILABLE",
                &message,
                &context.request_id,
            );
        }
    };
    if response.status().is_success()
        && files::audit(
            &audit,
            &context,
            &format!("files.agent.{action}.completed"),
            &target,
        )
        .await
        .is_err()
    {
        // The operation already completed. Never tell callers it was unexecuted.
        eprintln!(
            "file agent completion audit failed request_id={}",
            context.request_id
        );
    }
    response
}

#[cfg(unix)]
async fn exchange(
    socket: &std::path::Path,
    context: &RequestContext,
    request: Request<Body>,
) -> Result<Response, String> {
    use hyper_util::rt::TokioIo;
    let stream = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::net::UnixStream::connect(socket),
    )
    .await
    .map_err(|_| "本机文件服务连接超时，请检查 yaya-file-agent 服务")?
    .map_err(|_| "本机文件服务不可用，请检查 yaya-file-agent 服务及 Socket 权限")?;
    // A group-writable socket path must not allow a replacement service to impersonate root.
    if stream
        .peer_cred()
        .map_err(|_| "无法验证本机文件服务身份")?
        .uid()
        != 0
    {
        return Err("本机文件服务身份校验失败，拒绝连接".into());
    }
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|_| "本机文件服务协议连接失败")?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let (parts, body) = request.into_parts();
    let mut outgoing = Request::builder()
        .method(parts.method)
        .uri(
            parts
                .uri
                .path_and_query()
                .map(|q| q.as_str())
                .unwrap_or("/"),
        )
        .header(header::HOST, "files.local")
        .header(PROTOCOL_HEADER, PROTOCOL)
        .header("x-yaya-actor", &context.user.user_id)
        .header("x-request-id", &context.request_id);
    for name in [header::CONTENT_TYPE, header::ORIGIN] {
        if let Some(value) = parts.headers.get(&name) {
            outgoing = outgoing.header(name, value);
        }
    }
    let response = tokio::time::timeout(
        Duration::from_secs(15 * 60),
        sender.send_request(outgoing.body(body).map_err(|_| "文件请求构建失败")?),
    )
    .await
    .map_err(|_| "文件操作超时，请刷新核对实际状态后再重试")?
    .map_err(|_| "本机文件服务连接中断，请刷新核对实际状态")?;
    if response
        .headers()
        .get(PROTOCOL_HEADER)
        .and_then(|h| h.to_str().ok())
        != Some(PROTOCOL)
    {
        return Err("本机文件服务版本不匹配，请重新发布平台".into());
    }
    let (parts, body) = response.into_parts();
    let mut response = Response::builder().status(parts.status);
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_DISPOSITION,
        header::CACHE_CONTROL,
        header::HeaderName::from_static("x-content-type-options"),
    ] {
        if let Some(value) = parts.headers.get(&name) {
            response = response.header(name, value);
        }
    }
    response
        .body(Body::new(body))
        .map_err(|_| "文件响应构建失败".into())
}

#[cfg(not(unix))]
async fn exchange(
    _: &std::path::Path,
    _: &RequestContext,
    _: Request<Body>,
) -> Result<Response, String> {
    Err("当前环境不支持 Unix Socket 文件服务；Windows 请使用服务账号权限并移除 YAYA_FILE_AGENT_SOCKET 配置".into())
}

pub async fn run() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        server::run().await
    }
    #[cfg(not(unix))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "File agent currently requires Linux/Unix; Windows uses the API service account",
        ))
    }
}

#[cfg(unix)]
mod server {
    use super::*;
    use futures_util::StreamExt;
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    use tokio::net::{UnixListener, UnixStream};
    use tokio::sync::Semaphore;

    struct TrustedListener {
        inner: UnixListener,
        uid: u32,
    }
    impl axum::serve::Listener for TrustedListener {
        type Io = UnixStream;
        type Addr = tokio::net::unix::SocketAddr;
        async fn accept(&mut self) -> (Self::Io, Self::Addr) {
            loop {
                match self.inner.accept().await {
                    Ok((stream, addr)) if stream.peer_cred().is_ok_and(|c| c.uid() == self.uid) => {
                        return (stream, addr);
                    }
                    Ok(_) => {
                        eprintln!("file agent rejected Unix peer");
                    }
                    Err(e) => {
                        eprintln!("file agent accept: {e}");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }
        fn local_addr(&self) -> std::io::Result<Self::Addr> {
            self.inner.local_addr()
        }
    }

    async fn context(
        Extension(limit): Extension<std::sync::Arc<Semaphore>>,
        mut request: Request<Body>,
        next: Next,
    ) -> Response {
        let request_id = request
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let actor = request
            .headers()
            .get("x-yaya-actor")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let health = request.uri().path() == "/v1/health" && request.method() == Method::GET;
        let valid = request
            .headers()
            .get(PROTOCOL_HEADER)
            .and_then(|v| v.to_str().ok())
            == Some(PROTOCOL);
        let mut response = if !health
            && (!valid
                || actor.is_empty()
                || actor.len() > 256
                || request_id.is_empty()
                || request_id.len() > 256)
        {
            failure(
                StatusCode::FORBIDDEN,
                "FILE_AGENT_CONTEXT_REQUIRED",
                "文件服务仅接受平台内部请求",
                &request_id,
            )
        } else {
            let Ok(permit) = limit.try_acquire_owned() else {
                let mut response = failure(
                    StatusCode::TOO_MANY_REQUESTS,
                    "FILE_AGENT_BUSY",
                    "文件任务繁忙，请稍后重试",
                    &request_id,
                );
                response
                    .headers_mut()
                    .insert(PROTOCOL_HEADER, HeaderValue::from_static(PROTOCOL));
                return response;
            };
            request.extensions_mut().insert(RequestContext {
                user: AuthenticatedUser {
                    user_id: actor.clone(),
                    role: "platform_admin".into(),
                    organization_id: String::new(),
                },
                request_id: request_id.clone(),
            });
            let path = request.uri().to_string();
            let response =
                match tokio::time::timeout(Duration::from_secs(15 * 60), next.run(request)).await {
                    Ok(response) => response,
                    Err(_) => failure(
                        StatusCode::REQUEST_TIMEOUT,
                        "FILE_AGENT_TIMEOUT",
                        "文件操作超时，请刷新核对实际状态",
                        &request_id,
                    ),
                };
            eprintln!(
                "{}",
                json!({"actor":actor,"requestId":request_id,"resource":path,"status":response.status().as_u16()})
            );
            let (parts, body) = response.into_parts();
            Response::from_parts(
                parts,
                Body::from_stream(body.into_data_stream().map(move |chunk| {
                    let _ = &permit;
                    chunk
                })),
            )
        };
        response
            .headers_mut()
            .insert(PROTOCOL_HEADER, HeaderValue::from_static(PROTOCOL));
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    }

    pub(super) async fn run() -> std::io::Result<()> {
        let invalid =
            |message: &str| std::io::Error::new(std::io::ErrorKind::InvalidInput, message);
        let socket = socket_path().ok_or_else(|| invalid("YAYA_FILE_AGENT_SOCKET is required"))?;
        let allowed_uid: u32 = std::env::var("YAYA_FILE_AGENT_ALLOWED_UID")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v| *v != 0)
            .ok_or_else(|| {
                invalid("YAYA_FILE_AGENT_ALLOWED_UID must identify the non-root API account")
            })?;
        if !socket.is_absolute() {
            return Err(invalid("File agent socket must be absolute"));
        }
        files::validate_root().map_err(|e| invalid(&e))?;
        let parent = socket
            .parent()
            .ok_or_else(|| invalid("Missing socket parent"))?;
        let metadata = std::fs::symlink_metadata(parent)?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0 {
            return Err(invalid(
                "Socket parent must be a root-owned directory without group/world write permission",
            ));
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&socket) {
            if !metadata.file_type().is_socket() || metadata.uid() != 0 {
                return Err(invalid(
                    "Refusing to replace a non-root or non-socket entry",
                ));
            }
            match UnixStream::connect(&socket).await {
                Ok(_) => return Err(invalid("File agent socket is already active")),
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(&socket)?
                }
                Err(e) => return Err(e),
            }
        }
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o660))?;
        let app = files::routes::<()>()
            .route(
                "/v1/health",
                get(|| async { ok(json!({"protocol":PROTOCOL,"ready":true})) }),
            )
            .layer(Extension(files::FileAudit::Agent))
            .layer(middleware::from_fn(context))
            .layer(Extension(std::sync::Arc::new(Semaphore::new(8))));
        eprintln!(
            "file agent ready socket={} allowed_uid={allowed_uid}",
            socket.display()
        );
        axum::serve(
            TrustedListener {
                inner: listener,
                uid: allowed_uid,
            },
            app,
        )
        .with_graceful_shutdown(async {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM");
            tokio::select! { _ = terminate.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rpc_allowlist_does_not_include_shell_or_other_api_routes() {
        assert_eq!(
            operation(&Method::POST, "/api/ops/local/files/mutate"),
            Some("mutate")
        );
        for path in [
            "/api/users",
            "/api/ops/local/containers",
            "/api/ops/local/files/exec",
            "/v1/health",
        ] {
            assert!(operation(&Method::GET, path).is_none());
        }
        assert!(operation(&Method::DELETE, "/api/ops/local/files").is_none());
    }
}
