//! Customer platform instances deployed to the current environment Docker daemon.
mod authorization;
mod desktop;
mod public_key;
mod support_access;
pub(crate) mod lifecycle;
mod environment;
mod local_docker;
pub(crate) mod networks;
pub(crate) use local_docker::diagnostic as docker_diagnostic;
mod registries;
pub(crate) mod runtime_state;
use super::*;

type Failure = (StatusCode, Json<Envelope<()>>);
fn db_error(_: impl std::fmt::Display) -> Failure {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "平台交付数据读取或保存失败",
    )
}
fn bad(message: &str) -> Failure {
    error(StatusCode::BAD_REQUEST, message)
}

pub(super) fn initialize(path: &FilePath) -> Result<(), String> {
    let mut db = ops::db::open(path).map_err(|e| e.to_string())?;
    // SQLite table rebuilds require foreign keys disabled before the transaction.
    db.execute_batch("PRAGMA foreign_keys=OFF; PRAGMA legacy_alter_table=ON;")
        .map_err(|e| e.to_string())?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS platform_delivery_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL);").map_err(|e|e.to_string())?;
    for (version, source) in [
        (
            1,
            include_str!("../migrations/delivery/0001_builds_and_instances.sql"),
        ),
        (
            2,
            include_str!("../migrations/delivery/0002_local_docker_deployment.sql"),
        ),
        (
            3,
            include_str!("../migrations/delivery/0003_registry_connections.sql"),
        ),
        (
            4,
            include_str!("../migrations/delivery/0004_runtime_state.sql"),
        ),
        (
            5,
            include_str!("../migrations/delivery/0005_instance_container_sequences.sql"),
        ),
        (
            6,
            include_str!("../migrations/delivery/0006_instance_network.sql"),
        ),
        (
            7,
            include_str!("../migrations/delivery/0007_instance_authorization.sql"),
        ),
        (8, include_str!("../migrations/delivery/0008_container_revisions.sql")),
        (9, include_str!("../migrations/delivery/0009_support_access.sql")),
    ] {
        let sql = source.replace("\r\n", "\n");
        let digest = format!("{:x}", Sha256::digest(sql.as_bytes()));
        let saved: Option<String> = tx
            .query_row(
                "SELECT checksum FROM platform_delivery_migrations WHERE version=?",
                [version],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match saved {
            Some(value) if value != digest => {
                return Err(format!(
                    "Platform delivery migration checksum mismatch (version {version})"
                ));
            }
            Some(_) => {}
            None => {
                tx.execute_batch(&sql).map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO platform_delivery_migrations VALUES(?,?)",
                    params![version, digest],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    let invalid: bool = tx
        .prepare("PRAGMA foreign_key_check")
        .map_err(|e| e.to_string())?
        .exists([])
        .map_err(|e| e.to_string())?;
    if invalid {
        return Err("Platform delivery migration foreign key check failed".into());
    }
    tx.commit().map_err(|e| e.to_string())
}
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .merge(support_access::router())
        .merge(lifecycle::router())
        .route(
            "/api/platform-registries",
            get(registries::list).post(registries::create),
        )
        .route(
            "/api/platform-registries/{id}",
            axum::routing::put(registries::update).delete(registries::remove),
        )
        .route(
            "/api/customers/{customer_id}/platform-instances",
            get(customer_instances),
        )
        .route(
            "/api/platform-instances/{id}/sync-license",
            post(authorization::sync),
        )
        .route(
            "/api/platform-instances/{id}/license-syncs",
            get(authorization::history),
        )
        .route("/api/platform-runtime", get(local_docker::runtime))
        .route("/api/platform-networks", get(networks::list))
        .route(
            "/api/platform-instances/{id}/deploy",
            post(local_docker::deploy),
        )
        .route(
            "/api/platform-instances/{id}/deployments",
            get(local_docker::deployments),
        )
        .route(
            "/api/platform-instances/{id}/desktop-installer",
            post(desktop::download),
        )
        .route(
            "/api/platform-instances",
            get(instances).post(create_instance),
        )
        .route("/api/platform-instances/options", get(options))
        .route(
            "/api/platform-instances/{id}",
            get(instance).put(update_instance),
        )
        .route(
            "/api/platform-instances/{id}/archive",
            post(archive_instance),
        )
}
pub(super) fn recover(path: &FilePath) -> Result<(), String> {
    local_docker::recover(path)
}
async fn admin(headers: &HeaderMap, state: &AppState) -> Result<AuthenticatedUser, Failure> {
    let user = require_authenticated(headers, state).await?;
    if user.role != "platform_admin" {
        return Err(error(
            StatusCode::FORBIDDEN,
            "仅平台超级管理员可管理平台构建与实例",
        ));
    }
    Ok(user)
}
async fn work<T: Send + 'static>(
    state: AppState,
    f: impl FnOnce(&mut Connection) -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(move || {
        let mut db = ops::db::open(&state.database_path).map_err(db_error)?;
        f(&mut db)
    })
    .await
    .map_err(db_error)?
}
fn event(db: &Connection, id: &str, action: &str, actor: &str) -> Result<(), Failure> {
    db.execute("INSERT INTO platform_delivery_events(resource_id,action,actor_id,created_at) VALUES(?,?,?,?)",params![id,action,actor,Utc::now().timestamp_millis()]).map_err(db_error)?;
    Ok(())
}
fn default_bridge() -> String {
    "bridge".into()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstanceInput {
    name: String,
    customer_id: String,
    package_id: String,
    #[serde(default)]
    license_id: Option<String>,
    #[serde(default)]
    container_name: String,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default = "default_bridge")]
    network_mode: String,
    #[serde(default = "default_bridge")]
    network_name: String,
    web_port: u16,
    api_port: u16,
    #[serde(default)]
    public_url: String,
    #[serde(default)]
    notes: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceRecord {
    authorization_id: Option<String>,
    license_id: Option<String>,
    applied_license_id: Option<String>,
    license_sync_status: String,
    license_sync_error: Option<String>,
    license_synced_at: Option<i64>,
    instance_id: String,
    name: String,
    customer_id: String,
    customer_name: String,
    package_id: String,
    version: String,
    container_name: String,
    environment_keys: Vec<String>,
    container_id: Option<String>,
    image_id: Option<String>,
    network_mode: String,
    network_name: String,
    web_port: u16,
    api_port: u16,
    public_url: String,
    notes: String,
    status: String,
    runtime: Option<serde_json::Value>,
    created_at: i64,
    updated_at: i64,
}
const INSTANCE_SELECT: &str = "SELECT i.instance_id,i.name,i.customer_id,c.name,i.package_id,p.metadata,i.container_name,i.environment_encrypted,i.web_port,i.api_port,i.public_url,i.notes,i.status,i.deployment_status,i.created_at,i.updated_at,i.container_id,i.image_id,i.runtime_state,i.network_mode,i.network_name,i.authorization_id,i.license_id,i.applied_license_id,i.license_sync_status,i.license_sync_error,i.license_synced_at FROM platform_instances i JOIN customers c ON c.customer_id=i.customer_id JOIN platform_packages p ON p.package_id=i.package_id";
fn row_instance(r: &rusqlite::Row<'_>) -> rusqlite::Result<InstanceRecord> {
    let meta: String = r.get(5)?;
    let meta: serde_json::Value = serde_json::from_str(&meta).unwrap_or_default();
    let encrypted: Option<String> = r.get(7)?;
    // Environment keys are stored outside ciphertext for display; values never leave the API.
    let keys = encrypted
        .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
        .and_then(|v| v["keys"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let status: String = if r.get::<_, String>(12)? == "archived" {
        "archived".into()
    } else {
        r.get(13)?
    };
    Ok(InstanceRecord {
        authorization_id: r.get(21)?,
        license_id: r.get(22)?,
        applied_license_id: r.get(23)?,
        license_sync_status: r.get(24)?,
        license_sync_error: r.get(25)?,
        license_synced_at: r.get(26)?,
        instance_id: r.get(0)?,
        name: r.get(1)?,
        customer_id: r.get(2)?,
        customer_name: r.get(3)?,
        package_id: r.get(4)?,
        version: meta["version"].as_str().unwrap_or("unknown").into(),
        container_name: r.get(6)?,
        environment_keys: keys,
        network_mode: r.get(19)?,
        network_name: r.get(20)?,
        web_port: r.get(8)?,
        api_port: r.get(9)?,
        public_url: r.get(10)?,
        notes: r.get(11)?,
        status,
        runtime: r
            .get::<_, Option<String>>(18)?
            .and_then(|v| serde_json::from_str(&v).ok()),
        created_at: r.get(14)?,
        updated_at: r.get(15)?,
        container_id: r.get(16)?,
        image_id: r.get(17)?,
    })
}
async fn customer_instances(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(customer_id): Path<String>,
) -> Result<Json<Envelope<Vec<InstanceRecord>>>, Failure> {
    admin(&headers, &state).await?;
    let data = work(state, move |db| {
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=?)",
                [&customer_id],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if !exists {
            return Err(error(StatusCode::NOT_FOUND, "客户不存在"));
        }
        let mut q = db
            .prepare(&format!(
                "{INSTANCE_SELECT} WHERE i.customer_id=? ORDER BY i.created_at DESC,i.instance_id"
            ))
            .map_err(db_error)?;
        q.query_map([customer_id], row_instance)
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)
    })
    .await?;
    Ok(Json(success("客户平台实例", data)))
}
async fn instances(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<InstanceRecord>>>, Failure> {
    admin(&headers, &state).await?;
    let data = work(state, |db| {
        let mut q = db
            .prepare(&format!(
                "{INSTANCE_SELECT} ORDER BY i.created_at DESC,i.instance_id"
            ))
            .map_err(db_error)?;
        q.query_map([], row_instance)
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)
    })
    .await?;
    Ok(Json(success("平台实例", data)))
}
fn get_instance(db: &Connection, id: &str) -> Result<InstanceRecord, Failure> {
    db.query_row(
        &format!("{INSTANCE_SELECT} WHERE i.instance_id=?"),
        [id],
        row_instance,
    )
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| error(StatusCode::NOT_FOUND, "平台实例不存在"))
}
async fn instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<InstanceRecord>>, Failure> {
    admin(&headers, &state).await?;
    let data = work(state, move |db| get_instance(db, &id)).await?;
    Ok(Json(success("实例详情", data)))
}
fn validate_input(
    db: &Connection,
    input: &mut InstanceInput,
    exclude: &str,
) -> Result<(), Failure> {
    let (_, license) =
        authorization::eligible(db, &input.customer_id, input.license_id.as_deref())?;
    input.license_id = Some(license.license_id);
    networks::validate(&input.network_mode, &input.network_name)?;
    input.name = input.name.trim().into();
    input.public_url = input.public_url.trim().into();
    input.container_name = input.container_name.trim().into();
    if !regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]{1,62}$")
        .unwrap()
        .is_match(&input.container_name)
    {
        return Err(bad(
            "容器名须为 2–63 位字母、数字、点、下划线或短横线，且以字母或数字开头",
        ));
    }
    if let Some(text) = &input.environment {
        environment::parse(text)?;
    }
    if input.name.is_empty()
        || input.name.chars().count() > 80
        || input.notes.chars().count() > 2000
        || input.web_port < 1024
        || input.api_port < 1024
        || input.web_port == input.api_port
    {
        return Err(bad(
            "请输入 1–80 字的名称、最多 2000 字备注，以及两个不同的 1024–65535 端口",
        ));
    }
    if !input.public_url.is_empty() {
        let url = reqwest::Url::parse(&input.public_url)
            .map_err(|_| bad("访问地址必须为完整 HTTP/HTTPS 地址"))?;
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || input.public_url.len() > 500
        {
            return Err(bad("访问地址必须为不含凭据的 HTTP/HTTPS 地址"));
        }
    }
    let active: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=? AND status='active')",
            [&input.customer_id],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if !active {
        return Err(bad("请选择有效且启用的客户"));
    }
    let package: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM platform_packages WHERE package_id=? AND json_extract(metadata,'$.artifactType')='offline-image')",
            [&input.package_id],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if !package {
        return Err(bad("请选择已上传的离线镜像版本包"));
    }
    let conflict:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM platform_instances WHERE instance_id!=? AND status!='archived' AND (web_port IN (?,?) OR api_port IN (?,?)))",params![exclude,input.web_port,input.api_port,input.web_port,input.api_port],|r|r.get(0)).map_err(db_error)?;
    if conflict {
        return Err(error(
            StatusCode::CONFLICT,
            "当前 Docker 环境的端口已被其他平台实例占用",
        ));
    }
    let duplicate_container:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM platform_instances WHERE container_name=? AND instance_id!=?)",params![input.container_name,exclude],|r|r.get(0)).map_err(db_error)?;
    if duplicate_container {
        return Err(error(StatusCode::CONFLICT, "容器名称已被其他平台实例使用"));
    }
    Ok(())
}

fn next_container_name(
    db: &Connection,
    name: &str,
    docker_names: &HashSet<String>,
) -> Result<String, Failure> {
    let base = name.trim();
    if !regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*$")
        .unwrap()
        .is_match(base)
    {
        return Err(bad(
            "自动容器名要求实例名称仅包含字母、数字、点、下划线和短横线",
        ));
    }
    for number in 1..=1_000_000 {
        let suffix = format!("-{number}");
        let candidate = format!("{}{}", &base[..base.len().min(63 - suffix.len())], suffix);
        let reserved: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM platform_instances WHERE container_name=?)",
                [&candidate],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if !reserved && !docker_names.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(error(StatusCode::CONFLICT, "容器名称编号已耗尽"))
}
async fn create_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut input): Json<InstanceInput>,
) -> Result<Json<Envelope<InstanceRecord>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let docker_names = if input.container_name.trim().is_empty() {
        let text =
            crate::local_docker::read(&["ps", "--all", "--format", "{{.Names}}"], 5, 1024 * 1024)
                .await
                .map_err(|message| error(StatusCode::SERVICE_UNAVAILABLE, message))?;
        text.lines()
            .map(|line| line.trim().to_string())
            .collect::<HashSet<_>>()
    } else {
        HashSet::new()
    };
    networks::validate(&input.network_mode, &input.network_name)?;
    if input.network_mode != "bridge" || input.network_name != "bridge" {
        networks::verify(&input.network_mode, &input.network_name)
            .await
            .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    }
    let crypto_path = state.database_path.clone();
    let record=work(state,move|db| {
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        if input.container_name.trim().is_empty() { input.container_name=next_container_name(&tx,&input.name,&docker_names)?; }
        validate_input(&tx,&mut input,"")?;
        let id=Uuid::new_v4().to_string(); let now=Utc::now().timestamp_millis();
        let mut env=environment::prepare(None,input.environment.as_deref(),&input.public_url)?;
        environment::bind_customer(&tx,&input.customer_id,&id,&mut env)?;
        let encrypted=environment::encrypt(&crypto_path,&id,&env)?;
        tx.execute("INSERT INTO platform_instances(instance_id,name,customer_id,package_id,container_name,environment_encrypted,web_port,api_port,public_url,notes,created_by,created_at,updated_at,network_mode,network_name) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",params![id,input.name,input.customer_id,input.package_id,input.container_name,encrypted,input.web_port,input.api_port,input.public_url,input.notes,user.user_id,now,now,input.network_mode,input.network_name]).map_err(db_error)?;
        tx.execute("UPDATE platform_instances SET authorization_id=(SELECT authorization_id FROM customer_authorizations WHERE customer_id=?),license_id=?,license_sync_status='pending' WHERE instance_id=?",params![input.customer_id,input.license_id,id]).map_err(db_error)?;
        event(&tx,&id,"instance.created",&user.user_id)?;let record=get_instance(&tx,&id)?;tx.commit().map_err(db_error)?;Ok(record)
    }).await?;
    Ok(Json(success(
        "平台实例已创建，可以执行 Docker 部署",
        record,
    )))
}
async fn update_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut input): Json<InstanceInput>,
) -> Result<Json<Envelope<InstanceRecord>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    networks::validate(&input.network_mode, &input.network_name)?;
    if input.network_mode != "bridge" || input.network_name != "bridge" {
        networks::verify(&input.network_mode, &input.network_name)
            .await
            .map_err(|e| error(StatusCode::BAD_REQUEST, e))?;
    }
    let crypto_path = state.database_path.clone();
    let record=work(state,move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        let existing=get_instance(&tx,&id)?;
        if !["not_deployed","failed"].contains(&existing.status.as_str()) || existing.container_id.is_some() {return Err(error(StatusCode::CONFLICT,"已部署、部署中或已归档实例不可修改配置"));}
        if input.container_name.trim().is_empty() { input.container_name=existing.container_name; }
        validate_input(&tx,&mut input,&id)?;
        let encrypted:Option<String>=tx.query_row("SELECT environment_encrypted FROM platform_instances WHERE instance_id=?",[&id],|r|r.get(0)).map_err(db_error)?;
        let old=encrypted.map(|text|environment::decrypt(&crypto_path,&id,&text)).transpose()?;
        let mut env=environment::prepare(old,input.environment.as_deref(),&input.public_url)?;
        environment::bind_customer(&tx,&input.customer_id,&id,&mut env)?;
        let encrypted=environment::encrypt(&crypto_path,&id,&env)?;
        tx.execute("UPDATE platform_instances SET name=?,customer_id=?,package_id=?,node_id=NULL,container_name=?,environment_encrypted=?,web_port=?,api_port=?,public_url=?,notes=?,updated_at=?,network_mode=?,network_name=? WHERE instance_id=?",params![input.name,input.customer_id,input.package_id,input.container_name,encrypted,input.web_port,input.api_port,input.public_url,input.notes,Utc::now().timestamp_millis(),input.network_mode,input.network_name,id]).map_err(db_error)?;
        tx.execute("UPDATE platform_instances SET authorization_id=(SELECT authorization_id FROM customer_authorizations WHERE customer_id=?),license_id=?,license_sync_status='pending',license_sync_error=NULL WHERE instance_id=?",params![input.customer_id,input.license_id,id]).map_err(db_error)?;
        event(&tx,&id,"instance.updated",&user.user_id)?;let record=get_instance(&tx,&id)?;tx.commit().map_err(db_error)?;Ok(record)
    }).await?;
    Ok(Json(success("平台实例配置已保存", record)))
}
async fn archive_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<InstanceRecord>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let record = work(state, move |db| {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let existing = get_instance(&tx, &id)?;
        if !["not_deployed", "failed"].contains(&existing.status.as_str())
            || existing.container_id.is_some()
        {
            return Err(error(
                StatusCode::CONFLICT,
                "部署中、已有容器或已归档的实例不可归档",
            ));
        }
        tx.execute(
            "UPDATE platform_instances SET status='archived',updated_at=? WHERE instance_id=?",
            params![Utc::now().timestamp_millis(), id],
        )
        .map_err(db_error)?;
        event(&tx, &id, "instance.archived", &user.user_id)?;
        let record = get_instance(&tx, &id)?;
        tx.commit().map_err(db_error)?;
        Ok(record)
    })
    .await?;
    Ok(Json(success("平台实例已归档", record)))
}
async fn options(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<serde_json::Value>>, Failure> {
    admin(&headers, &state).await?;
    let data = work(state, |db| {
        let mut q = db
            .prepare("SELECT customer_id,name FROM customers WHERE status='active' ORDER BY name")
            .map_err(db_error)?;
        let customers = q
            .query_map([], |r| {
                Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?}))
            })
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?;
        let mut q = db
            .prepare("SELECT package_id,metadata FROM platform_packages WHERE json_extract(metadata,'$.artifactType')='offline-image' ORDER BY uploaded_at DESC")
            .map_err(db_error)?;
        let packages = q
            .query_map([], |r| {
                let raw: String = r.get(1)?;
                let meta: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":meta["version"],"environment":meta["imageEnvironment"],"deployment":meta["deployment"]}))
            })
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?;
        Ok(serde_json::json!({"customers":customers,"packages":packages}))
    })
    .await?;
    Ok(Json(success("实例配置选项", data)))
}

#[cfg(test)]
mod tests;
