//! First milestone: authenticated, administrator-only, read-only local operations.
mod container_create;
mod container_logs;
mod files;
pub(crate) mod files_agent;
pub mod db;
pub(crate) mod engine;
pub(crate) mod local_containers;
pub(crate) mod telemetry;
pub(crate) mod terminal;
#[cfg(test)]
mod tests;
use crate::{AppState, AuthenticatedUser};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use rusqlite::params;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use yaya_ops_protocol::{
    ContainerList, HostMonitorHello, HostMonitorReply, MetricsSnapshot, PROTOCOL_VERSION,
};

const ACTIONS: &[&str] = &["nodes.read", "containers.read", "metrics.read"];

pub struct OpsState {
    pub enabled: bool,
    pub socket: PathBuf,
    probe: Mutex<Option<(Instant, Result<HostMonitorHello, ProbeIssue>)>>,
}

#[derive(Clone, Copy)]
enum ProbeIssue {
    Offline,
    Incompatible,
}

impl OpsState {
    pub fn new(enabled: bool, socket: PathBuf) -> Self {
        Self {
            enabled,
            socket,
            probe: Mutex::new(None),
        }
    }
    pub fn from_env() -> Result<Self, String> {
        let enabled = match std::env::var("YAYA_OPS_ENABLED").as_deref() {
            Ok("true" | "1") => true,
            Ok("false" | "0") | Err(_) => false,
            _ => return Err("YAYA_OPS_ENABLED must be true/false or 1/0".into()),
        };
        Ok(Self::new(
            enabled,
            std::env::var_os("YAYA_HOST_MONITOR_SOCKET")
                .or_else(|| std::env::var_os("YAYA_OPS_AGENT_SOCKET"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/run/yaya-host-monitor/monitor.sock")),
        ))
    }
    async fn probe(&self) -> Result<HostMonitorHello, ProbeIssue> {
        let mut cached = self.probe.lock().await;
        if let Some((time, value)) = cached.as_ref() {
            if time.elapsed() < Duration::from_secs(5) {
                return value.clone();
            }
        }
        let reply =
            yaya_ops_protocol::local::get_json::<HostMonitorReply<HostMonitorHello>>(&self.socket, "/v1/hello")
                .await;
        let value = match reply.ok().and_then(|r| r.data) {
            Some(hello)
                if hello.protocol_version == PROTOCOL_VERSION && valid_node_id(&hello.node_id) =>
            {
                Ok(hello)
            }
            Some(_) => Err(ProbeIssue::Incompatible),
            None => Err(ProbeIssue::Offline),
        };
        *cached = Some((Instant::now(), value.clone()));
        value
    }
}

fn valid_node_id(value: &str) -> bool {
    value
        .strip_prefix("node_")
        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
}

pub fn initialize(path: &std::path::Path) -> Result<(), String> {
    let mut connection = db::open(path).map_err(|e| e.to_string())?;
    db::migrate(&mut connection)?;
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp_millis();
    tx.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) SELECT 'recovery-' || id,actor_user_id,'container.' || action || '.interrupted',container_id,503,'recovery-' || id,? FROM ops_container_actions WHERE status='running'",[now]).map_err(|e|e.to_string())?;
    tx.execute("UPDATE ops_container_actions SET status='interrupted',error='服务重启中断操作，请核对容器实际状态；不会自动重放',finished_at_ms=? WHERE status='running'", [now]).map_err(|e|e.to_string())?;
    tx.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) SELECT 'terminal-recovery-'||id,actor_user_id,'container.terminal.interrupted',container_id,503,id,? FROM ops_terminal_sessions WHERE status IN ('connecting','connected')",[now]).map_err(|e|e.to_string())?;
    tx.execute("UPDATE ops_terminal_sessions SET status='interrupted',reason='API 重启导致连接中断',disconnected_at_ms=? WHERE status IN ('connecting','connected')",[now]).map_err(|e|e.to_string())?;
    tx.execute("UPDATE ops_container_creations SET status='interrupted',error='API 重启中断创建，请核对实际容器状态；不会自动重建或删除数据卷',finished_at_ms=? WHERE status='running'",[now]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

#[derive(Clone)]
struct RequestContext {
    user: AuthenticatedUser,
    request_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorInfo {
    code: &'static str,
    request_id: String,
}

#[derive(Serialize)]
struct Reply<T: Serialize> {
    message: String,
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorInfo>,
}

fn ok<T: Serialize>(data: T) -> Json<Reply<T>> {
    Json(Reply {
        message: String::new(),
        data: Some(data),
        error: None,
    })
}
fn failure(status: StatusCode, code: &'static str, message: &str, request_id: &str) -> Response {
    (
        status,
        Json(Reply::<()> {
            message: message.into(),
            data: None,
            error: Some(ErrorInfo {
                code,
                request_id: request_id.into(),
            }),
        }),
    )
        .into_response()
}

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/ops/context", get(context))
        .merge(files::routes().layer(Extension(files::FileAudit::Api(state.clone())))
            .layer(middleware::from_fn_with_state(state.clone(), files_agent::forward)))
        .route(
            "/api/ops/local/containers",
            get(local_containers::list).post(container_create::create),
        )
        .route(
            "/api/ops/local/container-creations/{id}",
            get(container_create::task),
        )
        .route(
            "/api/ops/local/containers/{id}",
            get(local_containers::detail),
        )
        .route("/api/ops/local/inventory", get(local_containers::inventory))
        .route(
            "/api/ops/local/containers/{id}/logs",
            get(container_logs::snapshot),
        )
        .route(
            "/api/ops/local/containers/{id}/actions",
            get(local_containers::history).post(local_containers::action),
        )
        .route(
            "/api/ops/local/containers/{id}/terminal",
            get(terminal::upgrade),
        )
        .route(
            "/api/ops/local/containers/{id}/terminal-sessions",
            get(terminal::history),
        )
        .route(
            "/api/ops/local/containers/{id}/logs/live",
            get(container_logs::live),
        )
        .route(
            "/api/ops/local/containers/{id}/metrics",
            get(telemetry::history),
        )
        .route("/api/ops/local/actions/{id}", get(local_containers::task))
        .route("/api/ops/nodes", get(nodes))
        .route("/api/ops/nodes/{node_id}", get(node))
        .route("/api/ops/nodes/{node_id}/containers", get(containers))
        .route("/api/ops/nodes/{node_id}/metrics/latest", get(metrics))
        .layer(middleware::from_fn_with_state(state, gateway))
}

// The state is supplied by a dedicated outer layer in main, leaving business state unchanged.
pub async fn gateway(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = format!("req_{}", uuid::Uuid::new_v4().simple());
    let path = request.uri().path().to_owned();
    let method = request.method().to_string();
    let authenticated = crate::require_authenticated(request.headers(), &state).await;
    let mut actor = None;
    let mut response = match authenticated {
        Err(_) => failure(
            StatusCode::UNAUTHORIZED,
            "SESSION_EXPIRED",
            "请先登录",
            &request_id,
        ),
        Ok(user) => {
            let is_context = path == "/api/ops/context";
            actor = Some(user.user_id.clone());
            if !is_context && user.role != "platform_admin" {
                failure(
                    StatusCode::FORBIDDEN,
                    "OPS_FORBIDDEN",
                    "当前账号没有运维权限",
                    &request_id,
                )
            } else if !is_context && !path.starts_with("/api/ops/local/") && !state.ops.enabled {
                failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "OPS_DISABLED",
                    "运维功能尚未启用",
                    &request_id,
                )
            } else {
                request.extensions_mut().insert(RequestContext {
                    user,
                    request_id: request_id.clone(),
                });
                next.run(request).await
            }
        }
    };
    let status = response.status().as_u16();
    let logged_request_id = request_id.clone();
    let audit=db::blocking(state.database_path.clone(),move |connection| {
        connection.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) VALUES(?,?,?,?,?,?,?)",params![uuid::Uuid::new_v4().to_string(),actor,method,path,status,logged_request_id,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        // Bound retention without making read-only polling grow the database indefinitely.
        connection.execute("DELETE FROM ops_audit_events WHERE id IN (SELECT id FROM ops_audit_events WHERE created_at_ms<? ORDER BY created_at_ms LIMIT 100)",[chrono::Utc::now().timestamp_millis()-180*24*60*60*1000]).map_err(|e|e.to_string())?;
        Ok(())
    }).await;
    if audit.is_err() {
        eprintln!("ops audit unavailable request_id={request_id}");
    }
    response
        .headers_mut()
        .insert("x-request-id", HeaderValue::from_str(&request_id).unwrap());
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpsContext {
    environment_supported: bool,
    enabled: bool,
    allowed: bool,
    actions: Vec<&'static str>,
    writes_enabled: bool,
    host_terminal_enabled: bool,
    reason: Option<&'static str>,
}

async fn context(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
) -> Json<Reply<OpsContext>> {
    let allowed = context.user.role == "platform_admin";
    ok(OpsContext {
        environment_supported: cfg!(target_os = "linux"),
        enabled: state.ops.enabled,
        allowed,
        actions: if allowed && state.ops.enabled {
            ACTIONS.to_vec()
        } else {
            vec![]
        },
        writes_enabled: false,
        host_terminal_enabled: false,
        reason: if !allowed {
            Some("当前阶段仅平台超级管理员可访问运维工作区")
        } else if !state.ops.enabled {
            Some("当前环境尚未启用资源监控")
        } else {
            None
        },
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NodeView {
    #[serde(flatten)]
    hello: HostMonitorHello,
    status: String,
    last_seen_at_ms: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeList {
    items: Vec<NodeView>,
    connection_error: Option<&'static str>,
    connection_error_code: Option<&'static str>,
}

async fn probe_and_store(state: &AppState) -> Result<HostMonitorHello, ProbeIssue> {
    let hello = state.ops.probe().await?;
    let saved = hello.clone();
    let result=db::blocking(state.database_path.clone(),move |connection| {
        connection.execute("INSERT INTO ops_nodes(id,agent_identity,hello_json,last_seen_at_ms) VALUES(?,?,?,?) ON CONFLICT(id) DO UPDATE SET hello_json=excluded.hello_json,last_seen_at_ms=excluded.last_seen_at_ms",
            params![saved.node_id,saved.node_id,serde_json::to_string(&saved).map_err(|e|e.to_string())?,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        Ok(())
    }).await;
    if result.is_err() {
        return Err(ProbeIssue::Offline);
    }
    Ok(hello)
}

async fn nodes(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    match probe_and_store(&state).await {
        Ok(hello) => ok(NodeList {
            items: vec![NodeView {
                hello,
                status: "online".into(),
                last_seen_at_ms: chrono::Utc::now().timestamp_millis(),
            }],
            connection_error: None,
            connection_error_code: None,
        })
        .into_response(),
        Err(issue) => {
            let saved=db::blocking(state.database_path.clone(),|connection| {
                let mut statement=connection.prepare("SELECT hello_json,last_seen_at_ms FROM ops_nodes ORDER BY last_seen_at_ms DESC LIMIT 20").map_err(|e|e.to_string())?;
                let values=statement.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
                values.into_iter().map(|(json,last_seen_at_ms)|Ok(NodeView {hello:serde_json::from_str(&json).map_err(|e|e.to_string())?,status:"offline".into(),last_seen_at_ms})).collect::<Result<Vec<_>,String>>()
            }).await;
            match saved {
                Ok(items) => ok(NodeList {
                    items,
                    connection_error: Some(match issue {
                        ProbeIssue::Offline => "本机资源监控服务暂不可用，请稍后重试",
                        ProbeIssue::Incompatible => "资源监控服务版本不兼容",
                    }),
                    connection_error_code: Some(match issue {
                        ProbeIssue::Offline => "OPS_NODE_OFFLINE",
                        ProbeIssue::Incompatible => "OPS_AGENT_INCOMPATIBLE",
                    }),
                })
                .into_response(),
                Err(_) => failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "OPS_STORAGE_UNAVAILABLE",
                    "节点信息读取失败",
                    &context.request_id,
                ),
            }
        }
    }
}

async fn verify_node(
    state: &AppState,
    node_id: &str,
    context: &RequestContext,
) -> Result<HostMonitorHello, Response> {
    if !valid_node_id(node_id) {
        return Err(failure(
            StatusCode::NOT_FOUND,
            "OPS_RESOURCE_NOT_FOUND",
            "节点不存在",
            &context.request_id,
        ));
    }
    match probe_and_store(state).await {
        Ok(hello) if hello.node_id == node_id => Ok(hello),
        Ok(_) => Err(failure(
            StatusCode::NOT_FOUND,
            "OPS_RESOURCE_NOT_FOUND",
            "节点不存在",
            &context.request_id,
        )),
        Err(issue) => {
            let code = match issue {
                ProbeIssue::Offline => "OPS_NODE_OFFLINE",
                ProbeIssue::Incompatible => "OPS_AGENT_INCOMPATIBLE",
            };
            Err(failure(
                StatusCode::SERVICE_UNAVAILABLE,
                code,
                "本机资源监控服务暂不可用",
                &context.request_id,
            ))
        }
    }
}

async fn node(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Response {
    match verify_node(&state, &id, &context).await {
        Ok(hello) => ok(NodeView {
            hello,
            status: "online".into(),
            last_seen_at_ms: chrono::Utc::now().timestamp_millis(),
        })
        .into_response(),
        Err(response) => response,
    }
}

async fn monitor_read<T: DeserializeOwned>(
    state: &AppState,
    path: &str,
    context: &RequestContext,
) -> Result<T, Response> {
    let reply = yaya_ops_protocol::local::get_json::<HostMonitorReply<T>>(&state.ops.socket, path).await;
    match reply {
        Ok(reply) => match reply.data {
            Some(data) => Ok(data),
            None => Err(failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "OPS_CAPABILITY_UNAVAILABLE",
                reply.error.as_ref().map_or("节点能力不可用", |e| {
                    if e.code == "DOCKER_UNAVAILABLE" {
                        "Docker 不可用或版本不受支持"
                    } else {
                        "节点能力不可用"
                    }
                }),
                &context.request_id,
            )),
        },
        Err(_) => Err(failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "OPS_NODE_OFFLINE",
            "暂时无法读取本机资源监控数据",
            &context.request_id,
        )),
    }
}

async fn containers(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Response {
    if let Err(response) = verify_node(&state, &id, &context).await {
        return response;
    }
    match monitor_read::<ContainerList>(&state, "/v1/containers", &context).await {
        Ok(data) => ok(data).into_response(),
        Err(response) => response,
    }
}

async fn metrics(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Response {
    let hello = match verify_node(&state, &id, &context).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    match monitor_read::<MetricsSnapshot>(&state, "/v1/snapshot", &context).await {
        Ok(data) if data.node_id == id && data.boot_id == hello.boot_id => ok(data).into_response(),
        Ok(_) => failure(
            StatusCode::CONFLICT,
            "OPS_TARGET_CHANGED",
            "节点已经重启，请刷新节点状态",
            &context.request_id,
        ),
        Err(response) => response,
    }
}
