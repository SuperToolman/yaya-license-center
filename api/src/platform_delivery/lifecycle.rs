//! Durable, reversible instance revision changes. Old volumes are never mutated by a new revision.
mod backup;
mod executor;
mod recovery_files;
#[cfg(test)]
mod tests;
use super::*;
use crate::ops::engine;
use serde_json::{Value, json};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeploymentSpec {
    pub environment: Vec<EnvironmentSpec>,
    pub volumes: Vec<VolumeSpec>,
    pub web_port: u16,
    pub api_port: u16,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EnvironmentSpec {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VolumeSpec {
    pub key: String,
    pub destination: String,
}
impl DeploymentSpec {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.web_port == 0
            || self.api_port == 0
            || self.web_port == self.api_port
            || self.volumes.len() > 16
            || self.environment.len() > 512
        {
            return Err("部署清单端口或条目数量无效".into());
        }
        let mut keys = HashSet::new();
        let mut destinations = HashSet::new();
        for v in &self.volumes {
            if !safe_key(&v.key)
                || !v.destination.starts_with('/')
                || v.destination == "/"
                || v.destination.contains(['\0', ',', ':'])
                || v.destination.split('/').any(|p| p == "..")
                || !keys.insert(v.key.clone())
                || !destinations.insert(v.destination.clone())
            {
                return Err("存储用途标识或挂载路径无效/重复".into());
            }
        }
        let mut names = HashSet::new();
        for e in &self.environment {
            if !env_name(&e.name)
                || !names.insert(&e.name)
                || e.default
                    .as_ref()
                    .is_some_and(|s| s.len() > 32768 || s.contains('\0'))
            {
                return Err("环境变量部署声明无效".into());
            }
        }
        Ok(())
    }
}
fn safe_key(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn env_name(v: &str) -> bool {
    regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]{0,127}$")
        .unwrap()
        .is_match(v)
}
fn root(state: &AppState) -> PathBuf {
    state
        .database_path
        .parent()
        .unwrap_or(FilePath::new("."))
        .join("container-recovery")
}
fn seal(state: &AppState, id: &str, v: &Value) -> Result<String, Failure> {
    environment::encrypt(
        &state.database_path,
        id,
        &BTreeMap::from([("snapshot".into(), v.to_string())]),
    )
}
fn unseal(state: &AppState, id: &str, v: &str) -> Result<Value, String> {
    let m = environment::decrypt(&state.database_path, id, v).map_err(|(_, v)| v.0.message)?;
    serde_json::from_str(m.get("snapshot").ok_or("配置快照缺失")?)
        .map_err(|_| "配置快照无效".into())
}
fn db(state: &AppState) -> Result<Connection, String> {
    ops::db::open(&state.database_path).map_err(|e| e.to_string())
}
fn checked_id(id: &str) -> Result<(), Failure> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| bad("任务标识必须是 UUID"))
}
fn busy(db: &Connection, id: &str) -> rusqlite::Result<bool> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM container_version_tasks WHERE instance_id=? AND status IN ('queued','running','recovering','attention'))",[id],|r|r.get(0))
}
pub(crate) fn ensure_idle(db: &Connection, id: &str) -> Result<(), String> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='container_version_tasks')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists && busy(db, id).map_err(|e| e.to_string())? {
        return Err("实例正在备份、升级或恢复，或有待核对任务，请先处理版本任务".into());
    }
    Ok(())
}
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/platform-instances/{id}/versions", get(history))
        .route(
            "/api/platform-instances/{id}/recovery-points/{point}/download",
            get(recovery_files::download),
        )
        .route(
            "/api/platform-instances/{id}/recovery-points/{point}/verify",
            post(recovery_files::verify),
        )
        .route(
            "/api/platform-instances/{id}/recovery-points/{point}",
            axum::routing::delete(recovery_files::remove),
        )
        .route(
            "/api/platform-recovery/key-export",
            post(recovery_files::key_export),
        )
        .route(
            "/api/platform-instances/{id}/version-preview",
            post(preview),
        )
        .route("/api/platform-instances/{id}/version-tasks", post(submit))
        .route(
            "/api/platform-instances/{id}/version-reconcile",
            post(reconcile),
        )
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    task_id: String,
    kind: String,
    expected_container_id: String,
    #[serde(default)]
    package_id: Option<String>,
    #[serde(default)]
    recovery_point_id: Option<String>,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    #[serde(default)]
    remove_environment: Vec<String>,
    #[serde(default)]
    acknowledge_data_restore: bool,
}
async fn inspect(id: &str) -> Result<Value, String> {
    if !(id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err("必须提供完整容器 ID".into());
    }
    engine::json("GET", &format!("/containers/{id}/json"), None).await
}
async fn target(
    state: &AppState,
    id: &str,
    expected: &str,
) -> Result<(InstanceRecord, Value), String> {
    let r = get_instance(&db(state)?, id).map_err(|(_, v)| v.0.message)?;
    if r.container_id.as_deref() != Some(expected)
        || r.status == "archived"
        || r.status == "deploying"
    {
        return Err("实例绑定已变化、归档或正在部署".into());
    }
    let v = inspect(expected).await?;
    let light = ops::local_containers::inspect(&[expected.into()]).await?;
    if light
        .first()
        .is_none_or(|v| v["instanceId"] != id || ops::local_containers::protected(v))
    {
        return Err("容器标签不匹配或受保护".into());
    }
    if !["running", "exited", "created"].contains(&v["State"]["Status"].as_str().unwrap_or("")) {
        return Err("请先将容器恢复到运行或停止状态".into());
    }
    Ok((r, v))
}
fn snapshot(record: &InstanceRecord, v: &Value) -> Value {
    let mut config = v["Config"].clone();
    config["Image"] = v["Image"].clone();
    config["HostConfig"] = v["HostConfig"].clone();
    // Reconnect only the configured network; never request an old endpoint's occupied IP/MAC.
    let mut endpoints = serde_json::Map::new();
    if let Some(network) = v["NetworkSettings"]["Networks"].get(&record.network_name) {
        let aliases: Vec<_> = network["Aliases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| {
                a.as_str().is_some_and(|a| {
                    a != v["Id"].as_str().unwrap_or("")
                        && !v["Id"].as_str().unwrap_or("").starts_with(a)
                })
            })
            .cloned()
            .collect();
        endpoints.insert(record.network_name.clone(), json!({"Aliases":aliases}));
    }
    config["NetworkingConfig"] = json!({"EndpointsConfig":endpoints});
    json!({"create":config,"mounts":v["Mounts"],"imageId":v["Image"],"containerId":v["Id"],"name":record.container_name,"packageId":record.package_id,"wasRunning":v["State"]["Running"]==true,"restartPolicy":v["HostConfig"]["RestartPolicy"],"licenseId":record.license_id,"environmentEncrypted":null})
}
fn package(state: &AppState, id: &str) -> Result<Value, String> {
    let text: String = db(state)?
        .query_row(
            "SELECT metadata FROM platform_packages WHERE package_id=?",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| "目标版本包不存在")?;
    serde_json::from_str(&text).map_err(|_| "版本元数据无效".into())
}
fn spec(meta: &Value) -> Result<DeploymentSpec, String> {
    let s: DeploymentSpec = serde_json::from_value(meta["deployment"].clone())
        .map_err(|_| "目标版本缺少部署清单，请使用新版脚本重新打包上传")?;
    s.validate()?;
    Ok(s)
}
fn env_map(v: &Value) -> BTreeMap<String, String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| {
            x.as_str()?
                .split_once('=')
                .map(|(k, v)| (k.into(), v.into()))
        })
        .collect()
}
fn resolve_environment(
    old: &BTreeMap<String, String>,
    spec: &DeploymentSpec,
    req: &Request,
) -> Result<BTreeMap<String, String>, String> {
    if req.environment.len() > 256 || req.remove_environment.len() > 256 {
        return Err("环境条目过多".into());
    }
    let mut result = BTreeMap::new();
    for e in &spec.environment {
        if req.remove_environment.contains(&e.name) {
            if e.required {
                return Err(format!("{} 为必填变量", e.name));
            }
            continue;
        }
        if let Some(v) = old.get(&e.name).cloned().or_else(|| e.default.clone()) {
            result.insert(e.name.clone(), v);
        }
    }
    // Existing explicit overrides that are absent from image ENV are opt-in via submitted keys.
    for (k, v) in &req.environment {
        if ["YAYA_PLATFORM_ADMIN_MOBILE", "YAYA_PLATFORM_INSTANCE_ID"].contains(&k.as_str()) {
            return Err("客户管理员手机号与实例身份由平台维护，不能手动覆盖".into());
        }
        if !env_name(k) || v.contains('\0') || v.len() > 32768 {
            return Err("环境变量内容无效".into());
        }
        result.insert(k.clone(), v.clone());
    }
    for e in &spec.environment {
        if e.required && result.get(&e.name).is_none_or(|s| s.is_empty()) {
            return Err(format!("请填写必填环境变量 {}", e.name));
        }
    }
    Ok(result)
}
async fn preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<Request>,
) -> Result<Json<Envelope<Value>>, Failure> {
    admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let (r, v) = target(&state, &id, &req.expected_container_id)
        .await
        .map_err(bad_owned)?;
    let meta = package(&state, req.package_id.as_deref().unwrap_or("")).map_err(bad_owned)?;
    let s = spec(&meta).map_err(bad_owned)?;
    let old = env_map(&v["Config"]["Env"]);
    let mut names: HashSet<String> = old.keys().cloned().collect();
    names.extend(s.environment.iter().map(|e| e.name.clone()));
    let environment:Vec<_>=names.into_iter().map(|name|{let e=s.environment.iter().find(|e|e.name==name);json!({"name":name,"current":old.contains_key(&name),"target":e.is_some(),"required":e.is_some_and(|e|e.required),"default":e.and_then(|e|e.default.as_ref()).filter(|_|!sensitive(&name)),"action":if !old.contains_key(&name){"added"}else if e.is_none(){"removed"}else{"inherited"}})}).collect();
    let source_meta = package(&state, &r.package_id).unwrap_or_default();
    let source_spec = spec(&source_meta).ok();
    let volumes:Vec<_>=s.volumes.iter().map(|volume|{let source=source_spec.as_ref().and_then(|s|s.volumes.iter().find(|v|v.key==volume.key)).map(|v|v.destination.as_str()).unwrap_or(volume.destination.as_str());let exists=v["Mounts"].as_array().is_some_and(|ms|ms.iter().any(|m|m["Destination"]==source));json!({"key":volume.key,"destination":volume.destination,"sourceDestination":if exists{Some(source)}else{None},"action":if exists{"clone"}else{"create"}})}).collect();
    Ok(Json(success(
        "版本变更预览",
        json!({"sourcePackageId":r.package_id,"targetPackageId":req.package_id,"targetImageId":meta["imageId"],"environment":environment,"volumes":volumes,"webPort":s.web_port,"apiPort":s.api_port,"message":"停止容器后创建加密恢复点，数据恢复到新卷；旧卷保留。"}),
    )))
}
fn sensitive(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "key",
        "secret",
        "token",
        "password",
        "credential",
        "database_url",
        "valkey_url",
    ]
    .iter()
    .any(|v| n.contains(v))
}
fn bad_owned(v: impl AsRef<str>) -> Failure {
    bad(v.as_ref())
}
async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<Request>,
) -> Result<Json<Envelope<Value>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    checked_id(&req.task_id)?;
    if !["backup", "upgrade", "restore"].contains(&req.kind.as_str()) {
        return Err(bad("不支持的版本操作"));
    }
    let mut connection = db(&state).map_err(bad_owned)?;
    let exists: Option<(String, String)> = connection
        .query_row(
            "SELECT instance_id,request_encrypted FROM container_version_tasks WHERE id=?",
            [&req.task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    if let Some((owner, saved)) = exists {
        if owner != id || unseal(&state, &req.task_id, &saved).map_err(bad_owned)? != json!(req) {
            return Err(error(StatusCode::CONFLICT, "幂等任务标识已用于其他请求"));
        }
        return Ok(Json(success("已存在此任务", json!({"taskId":req.task_id}))));
    }
    let (record, _) = target(&state, &id, &req.expected_container_id)
        .await
        .map_err(bad_owned)?;
    if req.kind == "upgrade" && !req.acknowledge_data_restore {
        return Err(bad("请确认停机、备份并升级"));
    }
    if req.kind == "upgrade" {
        let m = package(&state, req.package_id.as_deref().unwrap_or("")).map_err(bad_owned)?;
        spec(&m).map_err(bad_owned)?;
    }
    if req.kind == "restore" {
        if !req.acknowledge_data_restore {
            return Err(bad("请确认恢复点之后的数据不会出现在恢复后的系统中"));
        }
        let valid:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM container_recovery_points WHERE id=? AND instance_id=? AND status='ready')",params![req.recovery_point_id,id],|r|r.get(0)).map_err(db_error)?;
        if !valid {
            return Err(bad("恢复点不可用或不属于当前实例"));
        }
    }
    if req.kind != "backup" {
        authorization::eligible(&connection, &record.customer_id, None)?;
    }
    let encrypted = seal(&state, &req.task_id, &json!(req))?;
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(db_error)?;
    ensure_idle(&tx, &id).map_err(bad_owned)?;
    let live = get_instance(&tx, &id)?;
    if live.container_id != record.container_id {
        return Err(error(StatusCode::CONFLICT, "实例绑定已变化"));
    }
    let blockers:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM ops_container_actions WHERE container_id=? AND status='running') OR EXISTS(SELECT 1 FROM ops_terminal_sessions WHERE container_id=? AND status IN ('connecting','connected')) OR EXISTS(SELECT 1 FROM platform_license_syncs WHERE instance_id=? AND status='running')",params![req.expected_container_id,req.expected_container_id,id],|r|r.get(0)).map_err(db_error)?;
    if blockers {
        return Err(error(
            StatusCode::CONFLICT,
            "请先结束容器操作、终端连接和授权同步",
        ));
    }
    tx.execute("INSERT INTO container_version_tasks(id,instance_id,kind,status,stage,source_container_id,target_package_id,target_recovery_point_id,request_encrypted,actor_id,created_at) VALUES(?,?,?,'queued','queued',?,?,?,?,?,?)",params![req.task_id,id,req.kind,req.expected_container_id,req.package_id,req.recovery_point_id,encrypted,user.user_id,Utc::now().timestamp_millis()]).map_err(db_error)?;
    event(
        &tx,
        &id,
        &format!("version.{}.queued", req.kind),
        &user.user_id,
    )?;
    tx.commit().map_err(db_error)?;
    let result_id = req.task_id.clone();
    tokio::spawn(async move {
        executor::dispatch(state, id, req, headers).await;
    });
    Ok(Json(success("已提交版本任务", json!({"taskId":result_id}))))
}
async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<Value>>, Failure> {
    admin(&headers, &state).await?;
    let d = db(&state).map_err(bad_owned)?;
    get_instance(&d, &id)?;
    let mut q=d.prepare("SELECT id,container_id,image_id,package_id,created_at FROM container_revisions WHERE instance_id=? ORDER BY created_at DESC LIMIT 100").map_err(db_error)?;
    let revisions=q.query_map([&id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"containerId":r.get::<_,String>(1)?,"imageId":r.get::<_,String>(2)?,"packageId":r.get::<_,String>(3)?,"createdAt":r.get::<_,i64>(4)?}))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    let mut q=d.prepare("SELECT id,revision_id,status,size,created_at,error FROM container_recovery_points WHERE instance_id=? ORDER BY created_at DESC LIMIT 100").map_err(db_error)?;
    let points=q.query_map([&id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"revisionId":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"size":r.get::<_,i64>(3)?,"createdAt":r.get::<_,i64>(4)?,"error":r.get::<_,Option<String>>(5)?}))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    let mut q=d.prepare("SELECT id,kind,status,stage,recovery_point_id,target_container_id,created_at,error,recovery_error FROM container_version_tasks WHERE instance_id=? ORDER BY created_at DESC LIMIT 50").map_err(db_error)?;
    let tasks=q.query_map([&id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"kind":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"stage":r.get::<_,String>(3)?,"recoveryPointId":r.get::<_,Option<String>>(4)?,"targetContainerId":r.get::<_,Option<String>>(5)?,"createdAt":r.get::<_,i64>(6)?,"error":r.get::<_,Option<String>>(7)?,"recoveryError":r.get::<_,Option<String>>(8)?}))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    Ok(Json(success(
        "部署修订和恢复点",
        json!({"revisions":revisions,"recoveryPoints":points,"tasks":tasks}),
    )))
}
async fn reconcile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<()>>, Failure> {
    admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    executor::reconcile(&state, &id).await.map_err(bad_owned)?;
    Ok(Json(success("已核对并恢复原修订", ())))
}
pub(super) fn recover(path: &FilePath) -> Result<(), String> {
    let d = ops::db::open(path).map_err(|e| e.to_string())?;
    d.execute("UPDATE container_version_tasks SET status='attention',error='API 重启中断版本任务，请点击核对恢复原修订；不自动重放升级' WHERE status IN ('queued','running','recovering')",[]).map_err(|e|e.to_string())?;
    d.execute("UPDATE container_recovery_points SET status='failed',error='API 重启中断备份，恢复点不完整' WHERE status='creating'",[]).map_err(|e|e.to_string())?;
    Ok(())
}

pub(super) async fn capture_initial(state: &AppState, id: &str) -> Result<(), String> {
    let d = db(state)?;
    let record = get_instance(&d, id).map_err(|(_, v)| v.0.message)?;
    let container = record.container_id.as_deref().ok_or("容器尚未绑定")?;
    let v = inspect(container).await?;
    let revision = Uuid::new_v4().to_string();
    let mut snap = snapshot(&record, &v);
    let env: Option<String> = d
        .query_row(
            "SELECT environment_encrypted FROM platform_instances WHERE instance_id=?",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    snap["environmentEncrypted"] = json!(env);
    let encrypted = seal(state, &revision, &snap).map_err(|(_, v)| v.0.message)?;
    d.execute(
        "INSERT INTO container_revisions VALUES(?,?,?,?,?,?,?,?)",
        params![
            revision,
            id,
            container,
            v["Image"].as_str(),
            record.package_id,
            encrypted,
            Utc::now().timestamp_millis(),
            "initial-deployment"
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
