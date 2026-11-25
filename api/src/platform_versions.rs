//! Immutable, administrator-only container image delivery artifacts; independent of Ops enablement.
use super::*;
use axum::response::IntoResponse;
use std::io::BufReader;
use tokio::io::AsyncWriteExt;

const MAX_ARCHIVE: usize = 8 * 1024 * 1024 * 1024;
const MAX_IMAGE: u64 = 16 * 1024 * 1024 * 1024;
type ApiError = (StatusCode, Json<Envelope<()>>);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PackageRecord {
    package_id: String,
    version: String,
    filename: String,
    artifact_type: String,
    built_at: String,
    uploaded_at: i64,
    uploaded_by: String,
    #[serde(default)]
    uploaded_by_name: String,
    size: u64,
    sha256: String,
    file_count: usize,
    #[serde(default)]
    image_id: Option<String>,
    #[serde(default)]
    image_platform: Option<String>,
    #[serde(default)]
    image_environment: Option<Vec<String>>,
    #[serde(default)]
    deployment: Option<platform_delivery::lifecycle::DeploymentSpec>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Manifest {
    package_type: String,
    format_version: u32,
    artifact_type: String,
    version: String,
    package_id: String,
    built_at: String,
    image: Image,
    #[serde(default)]
    deployment: Option<platform_delivery::lifecycle::DeploymentSpec>,
}
impl Manifest {
    pub(super) fn image(&self) -> &Image {
        &self.image
    }
}
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Image {
    pub id: String,
    pub archive: String,
    pub size: u64,
    pub sha256: String,
    pub tag: String,
    pub platform: String,
    #[serde(default)]
    pub environment: Option<Vec<String>>,
}
pub(super) fn initialize(path: &FilePath) -> rusqlite::Result<()> {
    Connection::open(path)?.execute_batch("CREATE TABLE IF NOT EXISTS platform_packages(package_id TEXT PRIMARY KEY, sha256 TEXT NOT NULL UNIQUE, uploaded_at INTEGER NOT NULL, metadata TEXT NOT NULL); CREATE TABLE IF NOT EXISTS platform_package_events(id INTEGER PRIMARY KEY, package_id TEXT NOT NULL, user_id TEXT NOT NULL, action TEXT NOT NULL, created_at INTEGER NOT NULL);")
}
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/platform-versions", get(list).post(upload))
        .route("/api/platform-versions/{id}/download", get(download))
        .layer(DefaultBodyLimit::max(MAX_ARCHIVE + 1024 * 1024))
}
pub(super) fn storage(state: &AppState) -> PathBuf {
    std::env::var_os("YAYA_PLATFORM_PACKAGES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            state
                .database_path
                .parent()
                .unwrap_or(FilePath::new("."))
                .join("platform-packages")
        })
}
fn internal() -> ApiError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "平台包存储失败，请检查磁盘与服务日志",
    )
}
fn invalid(message: impl Into<String>) -> ApiError {
    error(StatusCode::BAD_REQUEST, message)
}
async fn authorize(headers: &HeaderMap, state: &AppState) -> Result<AuthenticatedUser, ApiError> {
    let user = require_authenticated(headers, state).await?;
    if user.role != "platform_admin" {
        return Err(error(
            StatusCode::FORBIDDEN,
            "仅平台超级管理员可访问平台版本包",
        ));
    }
    Ok(user)
}
pub(super) fn check_origin(headers: &HeaderMap) -> Result<(), ApiError> {
    // Browser multipart POSTs can be sent cross-site without CORS preflight.
    if let Some(origin) = headers.get("origin") {
        let value = origin.to_str().unwrap_or("");
        let configured = std::env::var("YAYA_OPERATION_CENTER_WEB_ORIGIN").unwrap_or_default();
        if !["http://localhost:8778", "http://127.0.0.1:8778"].contains(&value)
            && (configured.is_empty() || value != configured.trim_end_matches('/'))
        {
            return Err(error(StatusCode::FORBIDDEN, "不允许的上传来源"));
        }
    }
    Ok(())
}
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<PackageRecord>>>, ApiError> {
    authorize(&headers, &state).await?;
    let records = tokio::task::spawn_blocking(move || -> Result<Vec<PackageRecord>, String> {
        let db = Connection::open(state.database_path).map_err(|e| e.to_string())?;
        let mut query = db
            .prepare("SELECT p.metadata, u.display_name FROM platform_packages p LEFT JOIN users u ON u.user_id=json_extract(p.metadata,'$.uploadedBy') WHERE json_extract(p.metadata,'$.artifactType')='offline-image' ORDER BY p.uploaded_at DESC, p.package_id")
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            let (metadata, name) = row.map_err(|e| e.to_string())?;
            let mut record: PackageRecord = serde_json::from_str(&metadata).map_err(|e| e.to_string())?;
            record.uploaded_by_name = name.filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "未知用户".into());
            Ok(record)
        })
        .collect()
    })
    .await
    .map_err(|_| internal())?
    .map_err(|_| internal())?;
    Ok(Json(success("平台版本列表", records)))
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Envelope<PackageRecord>>, ApiError> {
    let user = authorize(&headers, &state).await?;
    check_origin(&headers)?;
    let root = storage(&state);
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|_| internal())?;
    let temporary = Temporary(root.join(format!(".upload-{}.partial", Uuid::new_v4())));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary.0)
        .await
        .map_err(|_| internal())?;
    let mut found = false;
    let mut size = 0usize;
    let mut digest = Sha256::new();
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| invalid("无效的上传请求或文件超出大小限制"))?
    {
        if field.name() != Some("file") || found {
            return Err(invalid("请只上传一个 file 字段的平台 ZIP 包"));
        }
        found = true;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| invalid("读取上传失败或包超过 8 GiB"))?
        {
            size = size
                .checked_add(chunk.len())
                .ok_or_else(|| invalid("包过大"))?;
            if size > MAX_ARCHIVE {
                return Err(error(StatusCode::PAYLOAD_TOO_LARGE, "平台包不得超过 8 GiB"));
            }
            digest.update(&chunk);
            file.write_all(&chunk).await.map_err(|_| internal())?;
        }
    }
    if !found || size == 0 {
        return Err(invalid("请选择平台 ZIP 包"));
    }
    file.sync_all().await.map_err(|_| internal())?;
    drop(file);
    let sha256 = format!("{:x}", digest.finalize());
    // The guard moves into the blocking task, which keeps cleanup working even
    // if the HTTP client disconnects while ZIP validation is running.
    let record = tokio::task::spawn_blocking(move || {
        let manifest = validate(&temporary.0).map_err(invalid)?;
        let record = PackageRecord {
            filename: format!("{}-{}.zip", manifest.version, manifest.package_id),
            version: manifest.version, package_id: manifest.package_id,
            artifact_type: manifest.artifact_type, built_at: manifest.built_at,
            uploaded_at: Utc::now().timestamp_millis(), uploaded_by: user.user_id,
            uploaded_by_name: String::new(),
            size: size as u64, sha256, file_count: 2,
            image_id: Some(manifest.image.id.clone()),
            image_platform: Some(manifest.image.platform.clone()),
            image_environment: manifest.image.environment.clone(),
            deployment: manifest.deployment.clone(),
        };
        let mut db = Connection::open(&state.database_path).map_err(|_|internal())?;
        let mut record = record;
        record.uploaded_by_name = db.query_row("SELECT display_name FROM users WHERE user_id=?", [&record.uploaded_by], |row| row.get::<_, String>(0)).optional().map_err(|_|internal())?.filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "未知用户".into());
        db.busy_timeout(std::time::Duration::from_secs(5)).map_err(|_|internal())?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_|internal())?;
        let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM platform_packages WHERE package_id=? OR sha256=?)", params![record.package_id, record.sha256], |r|r.get(0)).map_err(|_|internal())?;
        if duplicate { return Err(error(StatusCode::CONFLICT, "此平台包已上传（包 UUID 或 SHA-256 重复）")); }
        let metadata = serde_json::to_string(&record).map_err(|_|internal())?;
        tx.execute("INSERT INTO platform_packages VALUES(?,?,?,?)", params![record.package_id,record.sha256,record.uploaded_at,metadata]).map_err(|_|internal())?;
        tx.execute("INSERT INTO platform_package_events(package_id,user_id,action,created_at) VALUES(?,?,'upload',?)", params![record.package_id,record.uploaded_by,record.uploaded_at]).map_err(|_|internal())?;
        let target = root.join(&record.filename);
        // Refuse to replace even an orphan from a previous interrupted commit.
        fs::hard_link(&temporary.0, &target).map_err(|_|internal())?;
        if tx.commit().is_err() { let _=fs::remove_file(&target); return Err(internal()); }
        Ok(record)
    }).await.map_err(|_|internal())??;
    Ok(Json(success("平台包上传成功", record)))
}

pub(super) fn validate(path: &FilePath) -> Result<Manifest, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip =
        ZipArchive::new(BufReader::new(file)).map_err(|_| "不是有效 ZIP 平台包".to_string())?;
    if zip.len() != 2 {
        return Err("离线镜像包必须包含 manifest.json 和 image.tar".into());
    }
    let mut names = HashSet::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(|_| "ZIP 条目不可读取")?;
        if !matches!(entry.name(), "manifest.json" | "image.tar")
            || !names.insert(entry.name().to_owned())
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 != 0 && mode & 0o170000 != 0o100000)
        {
            return Err("ZIP 包含非法路径、链接、重复或多余文件".into());
        }
    }
    let manifest: Manifest = {
        let mut entry = zip
            .by_name("manifest.json")
            .map_err(|_| "缺少 manifest.json，请使用平台打包脚本")?;
        if entry.size() > 8 * 1024 * 1024 {
            return Err("manifest 过大".into());
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "manifest 读取失败")?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("manifest 过大".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "manifest 格式不正确")?
    };
    if manifest.package_type != "yaya-platform"
        || manifest.format_version != 3
        || manifest.artifact_type != "offline-image"
    {
        return Err("仅支持格式 v3 的完整离线镜像 ZIP，请重新打包".into());
    }
    let id = Uuid::parse_str(&manifest.package_id).map_err(|_| "包 UUID 不合法")?;
    if id.get_version_num() != 4 || id.to_string() != manifest.package_id {
        return Err("包 UUID 必须为标准 UUID v4".into());
    }
    if !regex::Regex::new(r"^\d+(?:\.\d+)+(?:[a-zA-Z][\w.-]*|-[\w.-]+)?$")
        .unwrap()
        .is_match(&manifest.version)
        || manifest.version.len() > 64
    {
        return Err("平台版本不合法".into());
    }
    DateTime::parse_from_rfc3339(&manifest.built_at).map_err(|_| "构建时间不合法")?;
    if let Some(spec) = &manifest.deployment { spec.validate()?; }
    let image = &manifest.image;
    if let Some(environment) = &image.environment {
        if environment.len() > 512 || environment.iter().map(|v| v.len()).sum::<usize>() > 32768 {
            return Err("镜像环境变量超过大小限制".into());
        }
        let mut keys = HashSet::new();
        let pattern = regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]{0,79}$").unwrap();
        for entry in environment {
            let (key, value) = entry.split_once('=').ok_or("镜像环境变量格式不正确")?;
            if !pattern.is_match(key) || value.contains(['\0', '\r', '\n']) || !keys.insert(key) {
                return Err("镜像环境变量名称重复或格式不正确".into());
            }
        }
    }
    if !regex::Regex::new(r"^sha256:[a-f0-9]{64}$")
        .unwrap()
        .is_match(&image.id)
        || !regex::Regex::new(r"^[a-f0-9]{64}$")
            .unwrap()
            .is_match(&image.sha256)
    {
        return Err("镜像 ID 或镜像归档 SHA-256 不合法".into());
    }
    if !matches!(image.platform.as_str(), "linux/amd64" | "linux/arm64")
        || image.tag != format!("yaya-platform:{}-{}", manifest.version, manifest.package_id)
        || image.archive != "image.tar"
        || image.size == 0
        || image.size > MAX_IMAGE
    {
        return Err("镜像标签、架构、归档名称或大小不合法".into());
    }
    let mut entry = zip
        .by_name("image.tar")
        .map_err(|_| "缺少完整镜像归档 image.tar")?;
    if entry.size() != image.size {
        return Err("image.tar 大小与清单不一致".into());
    }
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = entry
            .read(&mut buffer)
            .map_err(|_| "镜像归档解压或 CRC 校验失败")?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).ok_or("镜像归档大小溢出")?;
        if total > image.size {
            return Err("镜像归档实际展开大小超过声明".into());
        }
        hash.update(&buffer[..n]);
    }
    if total != image.size || format!("{:x}", hash.finalize()) != image.sha256 {
        return Err("image.tar SHA-256 或大小校验失败".into());
    }
    Ok(manifest)
}
// Only copy the fixed, validated payload name; never extract arbitrary ZIP paths.
pub(super) fn unpack_image(path: &FilePath, target: &FilePath) -> Result<Manifest, String> {
    let manifest = validate(path)?;
    let mut zip = ZipArchive::new(BufReader::new(
        fs::File::open(path).map_err(|_| "读取离线包失败")?,
    ))
    .map_err(|_| "读取离线 ZIP 失败")?;
    let mut entry = zip.by_name("image.tar").map_err(|_| "缺少镜像归档")?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|_| "创建临时镜像归档失败，请检查磁盘空间")?;
    let copied = std::io::copy(
        &mut entry.by_ref().take(manifest.image.size + 1),
        &mut output,
    )
    .map_err(|_| "展开镜像归档失败，请检查磁盘空间")?;
    if copied != manifest.image.size {
        return Err("展开镜像归档大小不一致".into());
    }
    output.sync_all().map_err(|_| "镜像归档写入磁盘失败")?;
    Ok(manifest)
}
async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let user = authorize(&headers, &state).await?;
    let db_path = state.database_path.clone();
    let record: PackageRecord = tokio::task::spawn_blocking(move || {
        let db = Connection::open(db_path).map_err(|_|internal())?;
        let metadata: Option<String> = db.query_row("SELECT metadata FROM platform_packages WHERE package_id=? AND json_extract(metadata,'$.artifactType')='offline-image'", [&id], |r|r.get(0)).optional().map_err(|_|internal())?;
        let record: PackageRecord = serde_json::from_str(&metadata.ok_or_else(||error(StatusCode::NOT_FOUND,"平台包不存在"))?).map_err(|_|internal())?;
        db.execute("INSERT INTO platform_package_events(package_id,user_id,action,created_at) VALUES(?,?,'download',?)", params![record.package_id,user.user_id,Utc::now().timestamp_millis()]).map_err(|_|internal())?;
        Ok::<_,ApiError>(record)
    }).await.map_err(|_|internal())??;
    let file = tokio::fs::File::open(storage(&state).join(&record.filename))
        .await
        .map_err(|_| {
            error(
                StatusCode::NOT_FOUND,
                "平台包文件缺失，请联系管理员检查存储",
            )
        })?;
    Ok((
        [
            ("content-type", "application/zip".into()),
            (
                "content-disposition",
                format!("attachment; filename=\"{}\"", record.filename),
            ),
            ("content-length", record.size.to_string()),
            ("cache-control", "private, no-store".into()),
            ("x-content-type-options", "nosniff".into()),
        ],
        Body::from_stream(tokio_util::io::ReaderStream::new(file)),
    )
        .into_response())
}

#[cfg(test)]
mod tests;
