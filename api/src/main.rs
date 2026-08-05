use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE, SET_COOKIE},
    },
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    signing_key: Arc<EncodingKey>,
    verification_key: Arc<DecodingKey>,
    admin_token: Arc<String>,
    database_path: PathBuf,
    sessions: Arc<RwLock<HashMap<String, i64>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueLicenseRequest {
    subject: String,
    modules: Vec<String>,
    platform_expires_at: Option<DateTime<Utc>>,
    platform_expires_in_days: Option<u32>,
    #[serde(default)]
    module_expires_at: HashMap<String, DateTime<Utc>>,
    // Accept the original single-expiry payload during the transition.
    expires_at: Option<DateTime<Utc>>,
    expires_in_days: Option<u32>,
    license_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IssueLicenseResponse {
    license: String,
    license_id: String,
    expires_at: i64,
    module_expires_at: HashMap<String, i64>,
    platform_status: String,
    module_statuses: HashMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LicenseRecord {
    license: String,
    license_id: String,
    subject: String,
    modules: Vec<String>,
    issued_at: i64,
    expires_at: i64,
    #[serde(default)]
    module_expires_at: HashMap<String, i64>,
    #[serde(default = "default_unactivated_status")]
    platform_status: String,
    #[serde(default)]
    module_statuses: HashMap<String, String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseView {
    license: String,
    license_id: String,
    subject: String,
    modules: Vec<String>,
    issued_at: i64,
    expires_at: i64,
    module_expires_at: HashMap<String, i64>,
    platform_status: String,
    module_statuses: HashMap<String, String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseClaims {
    license_id: String,
    subject: String,
    modules: Vec<String>,
    iat: usize,
    exp: usize,
    #[serde(default)]
    module_expires_at: HashMap<String, i64>,
    iss: String,
    aud: String,
}

#[derive(Serialize)]
struct LicenseStatus {
    valid: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    admin_token: String,
}

#[derive(Serialize)]
struct SessionStatus {
    authenticated: bool,
}

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[tokio::main]
async fn main() {
    let private_key = required_env("LICENSE_SIGNING_PRIVATE_KEY_PEM");
    let public_key = required_env("LICENSE_SIGNING_PUBLIC_KEY_PEM");
    let admin_token = required_env("LICENSE_CENTER_ADMIN_TOKEN");
    let legacy_revoked_path = std::env::var_os("LICENSE_CENTER_REVOKED_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".license-center-revocations.json"));
    let legacy_licenses_path = std::env::var_os("LICENSE_CENTER_LICENSES_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".license-center-licenses.json"));
    let database_path = std::env::var_os("LICENSE_CENTER_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".license-center.sqlite3"));
    initialize_database(&database_path, &legacy_licenses_path, &legacy_revoked_path)
        .expect("failed to initialize license database");
    let state = AppState {
        signing_key: Arc::new(
            EncodingKey::from_rsa_pem(private_key.as_bytes())
                .expect("LICENSE_SIGNING_PRIVATE_KEY_PEM must be a valid RSA PEM"),
        ),
        verification_key: Arc::new(
            DecodingKey::from_rsa_pem(public_key.as_bytes())
                .expect("LICENSE_SIGNING_PUBLIC_KEY_PEM must be a valid RSA PEM"),
        ),
        admin_token: Arc::new(admin_token),
        database_path,
        sessions: Arc::new(RwLock::new(HashMap::new())),
    };
    let app = Router::new()
        .route(
            "/healthz",
            get(|| async { Json(success("ok", LicenseStatus { valid: true })) }),
        )
        .route("/api/licenses", post(issue_license))
        .route("/api/licenses", get(list_licenses))
        .route("/api/session", get(session_status).post(create_session))
        .route("/api/licenses/{license_id}/revoke", post(revoke_license))
        .route("/api/licenses/{license_id}/destroy", post(destroy_license))
        .route(
            "/api/licenses/{license_id}/activate",
            post(activate_license),
        )
        .route("/api/licenses/{license_id}/status", get(license_status))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin([
                    HeaderValue::from_static("http://127.0.0.1:8778"),
                    HeaderValue::from_static("http://localhost:8778"),
                ])
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([AUTHORIZATION, CONTENT_TYPE])
                .allow_credentials(true),
        );
    let host = std::env::var("LICENSE_CENTER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("LICENSE_CENTER_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8779);
    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid LICENSE_CENTER_HOST or LICENSE_CENTER_PORT");
    println!("license center listening on http://{address}");
    axum::serve(
        tokio::net::TcpListener::bind(address)
            .await
            .expect("failed to bind license center"),
        app,
    )
    .await
    .expect("license center stopped unexpectedly");
}

async fn issue_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<IssueLicenseRequest>,
) -> Result<Json<Envelope<IssueLicenseResponse>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if request.subject.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "授权主体不能为空"));
    }
    let expires_at = match (
        request.platform_expires_in_days.or(request.expires_in_days),
        request.platform_expires_at.or(request.expires_at),
    ) {
        (Some(days), _) if days > 0 => Utc::now() + chrono::Duration::days(days.into()),
        (_, Some(timestamp)) if timestamp > Utc::now() => timestamp,
        _ => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "有效期必须为大于 0 的天数，或晚于当前时间",
            ));
        }
    };
    let license_id = request
        .license_id
        .unwrap_or_else(|| format!("lic_{}", Uuid::new_v4().simple()));
    let now = Utc::now().timestamp() as usize;
    let mut modules: Vec<String> = request
        .modules
        .into_iter()
        .filter(|module| !module.trim().is_empty())
        .collect();
    if !modules.iter().any(|module| module == "platform") {
        modules.insert(0, "platform".to_string());
    }
    modules.sort();
    modules.dedup();
    let mut module_expires_at = request
        .module_expires_at
        .into_iter()
        .filter(|(module, module_expires_at)| {
            modules.iter().any(|allowed| allowed == module)
                && module != "platform"
                && *module_expires_at > Utc::now()
        })
        .map(|(module, module_expires_at)| {
            (
                module,
                module_expires_at.timestamp().min(expires_at.timestamp()),
            )
        })
        .collect::<HashMap<_, _>>();
    for module in &modules {
        if module != "platform" {
            module_expires_at
                .entry(module.clone())
                .or_insert(expires_at.timestamp());
        }
    }
    let initial_module_statuses = modules
        .iter()
        .map(|module| (module.clone(), "unactivated".to_string()))
        .collect::<HashMap<_, _>>();
    let claims = LicenseClaims {
        license_id: license_id.clone(),
        subject: request.subject.trim().to_string(),
        modules: modules.clone(),
        iat: now,
        exp: expires_at.timestamp() as usize,
        module_expires_at: module_expires_at.clone(),
        iss: "yaya-license-center".to_string(),
        aud: "yaya-low-code".to_string(),
    };
    let license = encode(&Header::new(Algorithm::RS256), &claims, &state.signing_key)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证签发失败"))?;
    let response = IssueLicenseResponse {
        license: license.clone(),
        license_id: license_id.clone(),
        expires_at: claims.exp as i64,
        module_expires_at: module_expires_at.clone(),
        platform_status: "unactivated".to_string(),
        module_statuses: initial_module_statuses.clone(),
    };
    insert_license(
        &state.database_path,
        &LicenseRecord {
            license,
            license_id,
            subject: claims.subject.clone(),
            modules: claims.modules.clone(),
            issued_at: now as i64,
            expires_at: claims.exp as i64,
            module_expires_at,
            platform_status: "unactivated".to_string(),
            module_statuses: initial_module_statuses,
        },
    )
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证记录保存失败"))?;
    Ok(Json(success("许可证已签发", response)))
}

async fn list_licenses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<LicenseView>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let licenses = list_license_records(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证列表读取失败"))?;
    Ok(Json(success(
        "许可证列表已读取",
        licenses
            .iter()
            .map(|license| license_view(license))
            .collect(),
    )))
}

async fn revoke_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> Result<Json<Envelope<LicenseStatus>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    destroy_license_by_id(&state, &license_id).await?;
    Ok(Json(success(
        "许可证已吊销",
        LicenseStatus { valid: false },
    )))
}

async fn destroy_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> Result<Json<Envelope<LicenseStatus>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    destroy_license_by_id(&state, &license_id).await?;
    Ok(Json(success(
        "许可证已销毁",
        LicenseStatus { valid: false },
    )))
}

async fn activate_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> Result<Json<Envelope<LicenseStatus>>, (StatusCode, Json<Envelope<()>>)> {
    let claims = bearer_token(&headers)
        .and_then(|token| verify_license(token, &state).ok())
        .filter(|claims| claims.license_id == license_id)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "许可证无效或已过期"))?;
    let mut license = get_license_record(&state.database_path, &license_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证状态读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "许可证不存在"))?;
    if license.platform_status == "destroyed" {
        return Err(error(StatusCode::FORBIDDEN, "许可证已销毁"));
    }
    license.platform_status = "running".to_string();
    for module in &claims.modules {
        license.module_statuses.insert(
            module.clone(),
            if module_expiry(&license, module) > Utc::now().timestamp() {
                "running"
            } else {
                "expired"
            }
            .to_string(),
        );
    }
    if !activate_license_record(&state.database_path, &license)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证状态保存失败"))?
    {
        return Err(error(StatusCode::FORBIDDEN, "许可证已销毁"));
    }
    Ok(Json(success("许可证已激活", LicenseStatus { valid: true })))
}

async fn license_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> Json<Envelope<LicenseStatus>> {
    let token_valid = bearer_token(&headers)
        .and_then(|token| verify_license(token, &state).ok())
        .is_some_and(|claims| claims.license_id == license_id);
    let valid = token_valid
        && get_license_record(&state.database_path, &license_id)
            .ok()
            .flatten()
            .is_some_and(|license| license.platform_status != "destroyed");
    Json(success("许可证状态已读取", LicenseStatus { valid }))
}

async fn destroy_license_by_id(
    state: &AppState,
    license_id: &str,
) -> Result<(), (StatusCode, Json<Envelope<()>>)> {
    let mut license = get_license_record(&state.database_path, license_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证状态读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "许可证不存在"))?;
    license.platform_status = "destroyed".to_string();
    for module in &license.modules {
        license
            .module_statuses
            .insert(module.clone(), "destroyed".to_string());
    }
    update_license_record(&state.database_path, &license)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证状态保存失败"))?;
    Ok(())
}

fn license_view(license: &LicenseRecord) -> LicenseView {
    let now = Utc::now().timestamp();
    let platform_status = if license.platform_status == "destroyed" {
        "destroyed".to_string()
    } else if license.expires_at <= now {
        "expired".to_string()
    } else {
        license.platform_status.clone()
    };
    let module_statuses = license
        .modules
        .iter()
        .map(|module| {
            let status = if module == "platform" {
                platform_status.clone()
            } else if platform_status == "destroyed" {
                "destroyed".to_string()
            } else if platform_status == "expired" || module_expiry(license, module) <= now {
                "expired".to_string()
            } else {
                license
                    .module_statuses
                    .get(module)
                    .cloned()
                    .unwrap_or_else(|| "unactivated".to_string())
            };
            (module.clone(), status)
        })
        .collect();
    LicenseView {
        license: license.license.clone(),
        license_id: license.license_id.clone(),
        subject: license.subject.clone(),
        modules: license.modules.clone(),
        issued_at: license.issued_at,
        expires_at: license.expires_at,
        module_expires_at: license.module_expires_at.clone(),
        platform_status,
        module_statuses,
    }
}

fn module_expiry(license: &LicenseRecord, module: &str) -> i64 {
    license
        .module_expires_at
        .get(module)
        .copied()
        .unwrap_or(license.expires_at)
}

fn default_unactivated_status() -> String {
    "unactivated".to_string()
}

fn verify_license(token: &str, state: &AppState) -> Result<LicenseClaims, ()> {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&["yaya-license-center"]);
    validation.set_audience(&["yaya-low-code"]);
    decode::<LicenseClaims>(token, &state.verification_key, &validation)
        .map(|data| data.claims)
        .map_err(|_| ())
}

async fn session_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<SessionStatus>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    Ok(Json(success(
        "登录状态有效",
        SessionStatus {
            authenticated: true,
        },
    )))
}

async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<Envelope<SessionStatus>>), (StatusCode, Json<Envelope<()>>)> {
    if request.admin_token != state.admin_token.as_str() {
        return Err(error(StatusCode::UNAUTHORIZED, "管理员令牌无效"));
    }
    let session_id = Uuid::new_v4().simple().to_string();
    let expires_at = Utc::now().timestamp() + 7 * 24 * 60 * 60;
    state
        .sessions
        .write()
        .await
        .insert(session_id.clone(), expires_at);
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&format!(
            "license_center_session={session_id}; HttpOnly; Path=/; SameSite=Lax; Max-Age=604800"
        ))
        .expect("session id is valid for a cookie"),
    );
    Ok((
        headers,
        Json(success(
            "登录成功",
            SessionStatus {
                authenticated: true,
            },
        )),
    ))
}

async fn require_admin(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<(), (StatusCode, Json<Envelope<()>>)> {
    if bearer_token(headers).is_some_and(|token| token == state.admin_token.as_str()) {
        return Ok(());
    }
    let now = Utc::now().timestamp();
    let mut sessions = state.sessions.write().await;
    sessions.retain(|_, expires_at| *expires_at > now);
    if cookie_value(headers, "license_center_session")
        .and_then(|id| sessions.get(id))
        .is_some_and(|expires_at| *expires_at > now)
    {
        Ok(())
    } else {
        Err(error(StatusCode::UNAUTHORIZED, "请先输入管理员令牌"))
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then_some(value)
        })
}

fn initialize_database(
    database_path: &PathBuf,
    legacy_licenses_path: &PathBuf,
    legacy_revoked_path: &PathBuf,
) -> Result<(), String> {
    if let Some(parent) = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut connection = Connection::open(database_path).map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS licenses (
                license_id TEXT PRIMARY KEY NOT NULL,
                license TEXT NOT NULL,
                subject TEXT NOT NULL,
                modules_json TEXT NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                module_expires_at_json TEXT NOT NULL,
                platform_status TEXT NOT NULL,
                module_statuses_json TEXT NOT NULL
            );
            ",
        )
        .map_err(|error| error.to_string())?;

    let record_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM licenses", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if record_count != 0 {
        return Ok(());
    }

    let revoked = load_revoked(legacy_revoked_path);
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    for mut license in load_licenses(legacy_licenses_path) {
        if revoked.contains(&license.license_id) {
            license.platform_status = "destroyed".to_string();
            license.module_statuses = license
                .modules
                .iter()
                .map(|module| (module.clone(), "destroyed".to_string()))
                .collect();
        }
        insert_license_into_connection(&transaction, &license)
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

fn insert_license(database_path: &PathBuf, license: &LicenseRecord) -> rusqlite::Result<()> {
    let connection = Connection::open(database_path)?;
    insert_license_into_connection(&connection, license)
}

fn insert_license_into_connection(
    connection: &Connection,
    license: &LicenseRecord,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO licenses (
            license_id, license, subject, modules_json, issued_at, expires_at,
            module_expires_at_json, platform_status, module_statuses_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            license.license_id,
            license.license,
            license.subject,
            serde_json::to_string(&license.modules).expect("modules are serializable"),
            license.issued_at,
            license.expires_at,
            serde_json::to_string(&license.module_expires_at)
                .expect("module expiries are serializable"),
            license.platform_status,
            serde_json::to_string(&license.module_statuses)
                .expect("module statuses are serializable"),
        ],
    )?;
    Ok(())
}

fn list_license_records(database_path: &PathBuf) -> rusqlite::Result<Vec<LicenseRecord>> {
    let connection = Connection::open(database_path)?;
    let mut statement = connection.prepare(
        "SELECT license_id, license, subject, modules_json, issued_at, expires_at,
                module_expires_at_json, platform_status, module_statuses_json
         FROM licenses ORDER BY issued_at DESC",
    )?;
    statement.query_map([], license_record_from_row)?.collect()
}

fn get_license_record(
    database_path: &PathBuf,
    license_id: &str,
) -> rusqlite::Result<Option<LicenseRecord>> {
    let connection = Connection::open(database_path)?;
    connection
        .query_row(
            "SELECT license_id, license, subject, modules_json, issued_at, expires_at,
                    module_expires_at_json, platform_status, module_statuses_json
             FROM licenses WHERE license_id = ?",
            [license_id],
            license_record_from_row,
        )
        .optional()
}

fn update_license_record(database_path: &PathBuf, license: &LicenseRecord) -> rusqlite::Result<()> {
    let connection = Connection::open(database_path)?;
    connection.execute(
        "UPDATE licenses SET platform_status = ?, module_statuses_json = ? WHERE license_id = ?",
        params![
            license.platform_status,
            serde_json::to_string(&license.module_statuses)
                .expect("module statuses are serializable"),
            license.license_id,
        ],
    )?;
    Ok(())
}

// A destroyed license must never be revived by a concurrent activation request.
fn activate_license_record(
    database_path: &PathBuf,
    license: &LicenseRecord,
) -> rusqlite::Result<bool> {
    let connection = Connection::open(database_path)?;
    let updated = connection.execute(
        "UPDATE licenses SET platform_status = ?, module_statuses_json = ?
         WHERE license_id = ? AND platform_status <> 'destroyed'",
        params![
            license.platform_status,
            serde_json::to_string(&license.module_statuses)
                .expect("module statuses are serializable"),
            license.license_id,
        ],
    )?;
    Ok(updated == 1)
}

fn license_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LicenseRecord> {
    let modules_json: String = row.get(3)?;
    let module_expires_at_json: String = row.get(6)?;
    let module_statuses_json: String = row.get(8)?;
    Ok(LicenseRecord {
        license_id: row.get(0)?,
        license: row.get(1)?,
        subject: row.get(2)?,
        modules: serde_json::from_str(&modules_json).unwrap_or_default(),
        issued_at: row.get(4)?,
        expires_at: row.get(5)?,
        module_expires_at: serde_json::from_str(&module_expires_at_json).unwrap_or_default(),
        platform_status: row.get(7).unwrap_or_else(|_| default_unactivated_status()),
        module_statuses: serde_json::from_str(&module_statuses_json).unwrap_or_default(),
    })
}

fn load_revoked(path: &PathBuf) -> HashSet<String> {
    std::fs::read(path)
        .ok()
        .and_then(|content| serde_json::from_slice(&content).ok())
        .unwrap_or_default()
}

fn load_licenses(path: &PathBuf) -> Vec<LicenseRecord> {
    std::fs::read(path)
        .ok()
        .and_then(|content| serde_json::from_slice(&content).ok())
        .unwrap_or_default()
}

fn required_env(name: &str) -> String {
    if let Ok(value) = std::env::var(name) {
        return value;
    }

    let local_secret = match name {
        "LICENSE_SIGNING_PRIVATE_KEY_PEM" => Some("../secrets/private.pem"),
        "LICENSE_SIGNING_PUBLIC_KEY_PEM" => Some("../secrets/public.pem"),
        "LICENSE_CENTER_ADMIN_TOKEN" => Some("../secrets/admin-token.txt"),
        _ => None,
    };

    local_secret
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| {
            panic!("{name} must be configured, or its local development secret must exist")
        })
}
fn success<T: Serialize>(message: impl Into<String>, data: T) -> Envelope<T> {
    Envelope {
        code: 0,
        message: message.into(),
        data: Some(data),
    }
}
fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Envelope<()>>) {
    (
        status,
        Json(Envelope {
            code: status.as_u16() as i32,
            message: message.into(),
            data: None,
        }),
    )
}
