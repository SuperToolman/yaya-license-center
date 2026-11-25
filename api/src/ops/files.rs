//! Current-process filesystem, bounded by a capability directory; never invokes a shell.
use super::*;
use axum::{
    body::Body,
    extract::{Multipart, Query},
    http::{HeaderMap, header},
};
use cap_std::{ambient_authority, fs::Dir};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Component, Path as FsPath},
};

const UPLOAD_LIMIT: u64 = 500 * 1024 * 1024;
const PREVIEW_LIMIT: u64 = 256 * 1024;
#[derive(Clone)]
pub(super) enum FileAudit {
    Api(AppState),
    #[cfg_attr(not(unix), allow(dead_code))]
    Agent,
}

pub(super) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/api/ops/local/files", get(list))
        .route("/api/ops/local/files/preview", get(preview))
        .route("/api/ops/local/files/size", get(size))
        .route("/api/ops/local/files/properties", get(properties))
        .route("/api/ops/local/files/download", get(download))
        .route(
            "/api/ops/local/files/upload",
            axum::routing::post(upload)
                .layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route(
            "/api/ops/local/files/mutate",
            axum::routing::post(mutate).layer(axum::extract::DefaultBodyLimit::max(32 * 1024)),
        )
}

#[cfg(unix)]
pub(super) fn validate_root() -> Result<(), String> {
    Files::open().map(|_| ())
}
#[derive(Default, Deserialize)]
pub(super) struct Location {
    #[serde(default)]
    path: String,
    #[serde(default)]
    search: String,
    #[serde(default)]
    page: usize,
    #[serde(default)]
    check: bool,
}
struct Files {
    dir: Dir,
    root: String,
}
impl Files {
    fn open() -> Result<Self, String> {
        let root = std::env::var_os("YAYA_FILES_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if cfg!(windows) {
                    std::env::current_dir().unwrap_or_default()
                } else {
                    PathBuf::from("/")
                }
            });
        let root = std::fs::canonicalize(root).map_err(io_error)?;
        Ok(Self {
            dir: Dir::open_ambient_dir(&root, ambient_authority()).map_err(io_error)?,
            root: root.to_string_lossy().into_owned(),
        })
    }
    fn path(path: &str) -> Result<PathBuf, String> {
        if path.len() > 4096 || path.contains(['\0', '\\', ':']) {
            return Err("路径无效".into());
        }
        let relative = path.trim_start_matches('/');
        if FsPath::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err("禁止访问管理根目录以外的路径".into());
        }
        Ok(if relative.is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(relative)
        })
    }
    fn entry(&self, path: &str) -> Result<(PathBuf, cap_std::fs::Metadata), String> {
        let p = Self::path(path)?;
        let m = self.dir.symlink_metadata(&p).map_err(io_error)?;
        if m.is_symlink() {
            return Err("符号链接仅展示，不直接操作，请使用真实路径".into());
        }
        Ok((p, m))
    }
}
fn io_error(e: std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "当前服务账号没有此文件或目录的访问权限".into(),
        std::io::ErrorKind::NotFound => "文件或目录不存在，请刷新后重试".into(),
        std::io::ErrorKind::AlreadyExists => "同名文件或目录已存在，不会覆盖".into(),
        _ => format!("文件操作失败：{e}"),
    }
}
fn revision(m: &cap_std::fs::Metadata) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{}:{:?}:{:?}", m.len(), m.modified(), m.created()).as_bytes())
    )
}
fn child(parent: &str, name: &str) -> Result<String, String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', ':', '\0'])
        || name.len() > 255
    {
        return Err("名称无效，请勿包含路径分隔符".into());
    }
    let p = format!("{}/{name}", parent.trim_end_matches('/'));
    Files::path(&p)?;
    Ok(p)
}
fn delete_allowed(root: &str, path: &FsPath, directory: bool) -> bool {
    let parts: Vec<_> = path
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    if parts.is_empty() {
        return false;
    }
    if directory && root == "/" && parts.len() == 1 {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if [
            "bin", "boot", "dev", "etc", "home", "lib", "lib64", "media", "mnt", "opt", "proc",
            "root", "run", "sbin", "srv", "sys", "tmp", "usr", "var",
        ]
        .contains(&name)
        {
            return false;
        }
    }
    true
}
fn write_origin(headers: &HeaderMap) -> Result<(), String> {
    if !headers.contains_key("origin") || crate::platform_versions::check_origin(headers).is_err() {
        return Err("操作来源不允许，请从平台页面操作".into());
    }
    Ok(())
}
fn directory_size(dir: Dir) -> Result<Value, String> {
    let started = std::time::Instant::now();
    // Queue relative paths, not open directory handles: wide trees must not exhaust
    // the file service's descriptor budget before traversal actually begins.
    let mut pending = vec![(PathBuf::from("."), 0usize)];
    let (mut bytes, mut files, mut directories, mut skipped_links, mut entries) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    while let Some((relative, depth)) = pending.pop() {
        let current = dir.open_dir(&relative).map_err(io_error)?;
        for entry in current.entries().map_err(io_error)? {
            entries += 1;
            if entries > 100_000 || started.elapsed().as_secs() >= 20 {
                return Err(
                    "目录过大或计算超时（最多 100000 项 / 20 秒），未返回不完整的大小".into(),
                );
            }
            let entry = entry.map_err(io_error)?;
            let name = entry.file_name();
            let m = current.symlink_metadata(&name).map_err(io_error)?;
            if m.is_symlink() {
                skipped_links += 1;
            } else if m.is_file() {
                files += 1;
                bytes = bytes.checked_add(m.len()).ok_or("目录大小超出支持范围")?;
            } else if m.is_dir() {
                if depth >= 128 {
                    return Err("目录层级超过 128 层，无法完成计算".into());
                }
                directories += 1;
                pending.push((relative.join(name), depth + 1));
            }
        }
    }
    Ok(
        json!({"bytes":bytes,"files":files,"directories":directories,"skippedLinks":skipped_links,"calculatedAt":chrono::Utc::now().timestamp_millis()}),
    )
}
pub(super) async fn size(
    Extension(state): Extension<FileAudit>,
    Extension(context): Extension<RequestContext>,
    Query(q): Query<Location>,
) -> Response {
    let path = q.path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let f = Files::open()?;
        let (p, m) = f.entry(&q.path)?;
        if !m.is_dir() {
            return Err("目标不是目录".into());
        }
        directory_size(f.dir.open_dir(p).map_err(io_error)?)
    })
    .await
    .map_err(|_| "计算目录大小失败".to_string())
    .and_then(|v| v);
    if result.is_ok() {
        if let Err(e) = audit(&state, &context, "files.size", &path).await {
            return reply(Err(e), &context);
        }
    }
    reply(result, &context)
}
pub(super) async fn properties(
    Extension(context): Extension<RequestContext>,
    Query(q): Query<Location>,
) -> Response {
    let result = tokio::task::spawn_blocking(move || {
        let f = Files::open()?;
        let (p, m) = f.entry(&q.path)?;
        #[cfg(unix)]
        let (mode, owner) = { use cap_std::fs::MetadataExt; (format!("{:04o}", m.mode() & 0o7777), format!("{} / {}",m.uid(),m.gid())) };
        #[cfg(not(unix))]
        let (mode, owner) = (if m.permissions().readonly() { "只读" } else { "读写" }.to_string(), "—".to_string());
        Ok(json!({"name":p.file_name().map(|n|n.to_string_lossy().into_owned()).unwrap_or("/".into()),"path":q.path,"absolutePath":FsPath::new(&f.root).join(&p).to_string_lossy(),"kind":if m.is_dir(){"directory"}else if m.is_file(){"file"}else{"special"},"size":if m.is_file(){Some(m.len())}else{None},"mode":mode,"owner":owner,"modifiedAt":m.modified().ok().and_then(|t|t.into_std().duration_since(std::time::UNIX_EPOCH).ok()).map(|t|t.as_millis() as u64),"createdAt":m.created().ok().and_then(|t|t.into_std().duration_since(std::time::UNIX_EPOCH).ok()).map(|t|t.as_millis() as u64)}))
    }).await.map_err(|_| "读取属性失败".to_string()).and_then(|v| v);
    reply(result, &context)
}
fn rename_entry(
    dir: &Dir,
    old: &std::ffi::OsStr,
    name: &str,
    directory: bool,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let _ = directory;
        rustix::fs::renameat_with(dir, old, dir, name, rustix::fs::RenameFlags::NOREPLACE)
            .map_err(|e| io_error(e.into()))
    }
    #[cfg(not(target_os = "linux"))]
    {
        if directory {
            match dir.symlink_metadata(name) {
                Ok(_) => return Err("同名文件或目录已存在，不会覆盖".into()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_error(e)),
            }
            dir.rename(old, dir, name).map_err(io_error)
        } else {
            dir.hard_link(old, dir, name).map_err(io_error)?;
            if let Err(e) = dir.remove_file(old) {
                let _ = dir.remove_file(name);
                return Err(io_error(e));
            }
            Ok(())
        }
    }
}
pub(super) async fn audit(
    state: &FileAudit,
    context: &RequestContext,
    action: &str,
    path: &str,
) -> Result<(), String> {
    let (actor, request, action, path) = (
        context.user.user_id.clone(),
        context.request_id.clone(),
        action.to_string(),
        path.to_string(),
    );
    let FileAudit::Api(state) = state else {
        eprintln!(
            "{}",
            json!({"actor":actor,"requestId":request,"action":action,"path":path})
        );
        return Ok(());
    };
    db::blocking(state.database_path.clone(),move |d| {
        d.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) VALUES(?,?,?,?,200,?,?)",params![uuid::Uuid::new_v4().to_string(),actor,action,path,request,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?; Ok(())
    }).await
}
fn reply(result: Result<Value, String>, context: &RequestContext) -> Response {
    match result {
        Ok(v) => ok(v).into_response(),
        Err(e) => failure(
            StatusCode::BAD_REQUEST,
            "FILES_ERROR",
            &e,
            &context.request_id,
        ),
    }
}
pub(super) async fn list(
    Extension(context): Extension<RequestContext>,
    Query(q): Query<Location>,
) -> Response {
    let result = tokio::task::spawn_blocking(move || {
        let f = Files::open()?; let (p, meta) = f.entry(&q.path)?;
        if !meta.is_dir() { return Err("目标不是目录".into()); }
        let dir = f.dir.open_dir(p).map_err(io_error)?;
        let mut items = vec![]; let mut scanned = 0usize;
        for entry in dir.entries().map_err(io_error)? {
            scanned += 1; if scanned > 20000 { return Err("目录超过 20000 项，请使用系统工具整理后再查看".into()); }
            let entry = entry.map_err(io_error)?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue; };
            if !name.to_lowercase().contains(&q.search.to_lowercase()) { continue; }
            let m = dir.symlink_metadata(&name).map_err(io_error)?;
            let kind = if m.is_symlink() { "link" } else if m.is_dir() { "directory" } else if m.is_file() { "file" } else { "special" };
            #[cfg(unix)]
            let (mode,owner) = { use cap_std::fs::MetadataExt; (format!("{:04o}",m.mode() & 0o7777), format!("{} / {}",m.uid(),m.gid())) };
            #[cfg(not(unix))]
            let (mode,owner) = (if m.permissions().readonly(){"只读"}else{"读写"}.to_string(),"—".to_string());
            items.push(json!({"name":name,"path":child(&q.path,&name)?,"kind":kind,"size":if m.is_file(){Some(m.len())}else{None},"modifiedAt":m.modified().ok().and_then(|t|t.into_std().duration_since(std::time::UNIX_EPOCH).ok()).map(|t|t.as_millis() as u64),"mode":mode,"owner":owner,"revision":revision(&m),"canDelete":delete_allowed(&f.root,&Files::path(&child(&q.path,&name)?)?,m.is_dir())}));
        }
        items.sort_by(|a,b| (a["kind"]!="directory",a["name"].as_str()).cmp(&(b["kind"]!="directory",b["name"].as_str())));
        let total=items.len(); let start=q.page.saturating_mul(100).min(total);
        Ok(json!({"root":f.root,"path":format!("/{}",q.path.trim_matches('/')),"items":&items[start..(start+100).min(total)],"total":total,"page":q.page,"pageSize":100}))
    }).await.map_err(|_|"读取目录失败".to_string()).and_then(|v|v);
    reply(result, &context)
}
pub(super) async fn preview(
    Extension(state): Extension<FileAudit>,
    Extension(context): Extension<RequestContext>,
    Query(q): Query<Location>,
) -> Response {
    let path = q.path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let f = Files::open()?;
        let (p, m) = f.entry(&q.path)?;
        if !m.is_file() || m.len() > PREVIEW_LIMIT {
            return Err("仅支持预览不超过 256 KiB 的 UTF-8 文本文件，其他文件请下载".into());
        }
        let mut data = vec![];
        f.dir
            .open(p)
            .map_err(io_error)?
            .take(PREVIEW_LIMIT + 1)
            .read_to_end(&mut data)
            .map_err(io_error)?;
        if data.len() as u64 > PREVIEW_LIMIT || data.contains(&0) {
            return Err("文件过大或为二进制文件，请下载查看".into());
        }
        let text = String::from_utf8(data).map_err(|_| "文件不是 UTF-8 文本，请下载查看")?;
        Ok(json!({"text":text}))
    })
    .await
    .map_err(|_| "读取文件失败".to_string())
    .and_then(|v| v);
    if result.is_ok() {
        if let Err(e) = audit(&state, &context, "files.preview", &path).await {
            return reply(Err(e), &context);
        }
    }
    reply(result, &context)
}
pub(super) async fn download(
    Extension(state): Extension<FileAudit>,
    Extension(context): Extension<RequestContext>,
    Query(q): Query<Location>,
) -> Response {
    let result = async {
        let f = Files::open()?;
        let (p, m) = f.entry(&q.path)?;
        if !m.is_file() {
            return Err("仅支持下载普通文件".to_string());
        }
        let file = f.dir.open(&p).map_err(io_error)?.into_std();
        if !file.metadata().map_err(io_error)?.is_file() {
            return Err("目标文件类型已改变".into());
        }
        if q.check {
            return Ok(ok(json!({"readable":true})).into_response());
        }
        audit(&state, &context, "files.download", &q.path).await?;
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("download");
        let encoded: String = name
            .as_bytes()
            .iter()
            .map(|b| format!("%{b:02X}"))
            .collect();
        Ok(Response::builder()
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=download; filename*=UTF-8''{encoded}"),
            )
            .header("x-content-type-options", "nosniff")
            .body(Body::from_stream(tokio_util::io::ReaderStream::new(
                tokio::fs::File::from_std(file),
            )))
            .unwrap())
    }
    .await;
    match result {
        Ok(r) => r,
        Err(e) => reply(Err(e), &context),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Mutation {
    action: String,
    path: String,
    name: Option<String>,
    expected_revision: Option<String>,
}
pub(super) async fn mutate(
    Extension(state): Extension<FileAudit>,
    Extension(context): Extension<RequestContext>,
    headers: HeaderMap,
    Json(input): Json<Mutation>,
) -> Response {
    if let Err(e) = write_origin(&headers) {
        return failure(
            StatusCode::FORBIDDEN,
            "ORIGIN_DENIED",
            &e,
            &context.request_id,
        );
    }
    let action = input.action.clone();
    let path = input.path.clone();
    if let Err(e) = audit(
        &state,
        &context,
        &format!("files.{action}.requested"),
        &path,
    )
    .await
    {
        return reply(Err(e), &context);
    }
    let result = tokio::task::spawn_blocking(move || {
        let f = Files::open()?;
        match input.action.as_str() {
            "rename" => {
                let (p, m) = f.entry(&input.path)?;
                if !delete_allowed(&f.root, &p, m.is_dir()) || !(m.is_file() || m.is_dir()) {
                    return Err("此文件或目录不允许重命名".into());
                }
                if input.expected_revision.as_deref() != Some(revision(&m).as_str()) {
                    return Err("文件已变化，请刷新后重新重命名".into());
                }
                let name = input.name.ok_or("请填写新名称")?;
                child("/", &name)?;
                let parent = f
                    .dir
                    .open_dir(p.parent().unwrap_or(FsPath::new(".")))
                    .map_err(io_error)?;
                let old = p.file_name().ok_or("根目录不允许重命名")?;
                if old != std::ffi::OsStr::new(&name) {
                    rename_entry(&parent, old, &name, m.is_dir())?;
                }
            }
            "mkdir" | "create" => {
                let (parent, m) = f.entry(&input.path)?;
                if !m.is_dir() {
                    return Err("目标不是目录".into());
                }
                let dir = f.dir.open_dir(parent).map_err(io_error)?;
                let name = input.name.ok_or("请填写名称")?;
                child(&input.path, &name)?;
                if input.action == "mkdir" {
                    dir.create_dir(&name).map_err(io_error)?;
                } else {
                    dir.open_with(
                        &name,
                        cap_std::fs::OpenOptions::new().write(true).create_new(true),
                    )
                    .map_err(io_error)?;
                }
            }
            "delete" => {
                let (p, m) = f.entry(&input.path)?;
                if !delete_allowed(&f.root, &p, m.is_dir()) {
                    return Err("管理根目录或系统顶级目录受保护，禁止删除".into());
                }
                if input.expected_revision.as_deref() != Some(revision(&m).as_str()) {
                    return Err("文件已变化，请刷新后重新确认删除".into());
                }
                if m.is_dir() {
                    f.dir
                        .remove_dir(p)
                        .map_err(|_| "目录非空或无删除权限；请先处理目录内容".to_string())?;
                } else if m.is_file() {
                    f.dir.remove_file(p).map_err(io_error)?;
                } else {
                    return Err("不支持删除特殊文件".into());
                }
            }
            _ => return Err("未知文件操作".into()),
        }
        Ok(json!({"completed":true}))
    })
    .await
    .map_err(|_| "文件操作中断".to_string())
    .and_then(|v| v);
    let result = if result.is_ok() {
        audit(
            &state,
            &context,
            &format!("files.{action}.completed"),
            &path,
        )
        .await
        .map(|_| json!({"completed":true}))
    } else {
        result
    };
    reply(result, &context)
}
struct UploadCleanup {
    dir: Dir,
    name: String,
}
impl Drop for UploadCleanup {
    fn drop(&mut self) {
        let _ = self.dir.remove_file(&self.name);
    }
}
pub(super) async fn upload(
    Extension(state): Extension<FileAudit>,
    Extension(context): Extension<RequestContext>,
    headers: HeaderMap,
    Query(q): Query<Location>,
    mut multipart: Multipart,
) -> Response {
    if let Err(e) = write_origin(&headers) {
        return failure(
            StatusCode::FORBIDDEN,
            "ORIGIN_DENIED",
            &e,
            &context.request_id,
        );
    }
    let result = async {
        let f = Files::open()?;
        let (p, m) = f.entry(&q.path)?;
        if !m.is_dir() {
            return Err("目标不是目录".into());
        }
        let dir = f.dir.open_dir(p).map_err(io_error)?;
        let mut field = multipart
            .next_field()
            .await
            .map_err(|_| "上传内容无效")?
            .ok_or("请选择文件")?;
        let name = field.file_name().ok_or("缺少文件名")?.to_string();
        let path = child(&q.path, &name)?;
        audit(&state, &context, "files.upload.requested", &path).await?;
        // Publish only a complete upload; hard_link never replaces an existing target.
        let temp = format!(".yaya-upload-{}", uuid::Uuid::new_v4());
        let output = dir
            .open_with(
                &temp,
                cap_std::fs::OpenOptions::new().write(true).create_new(true),
            )
            .map_err(io_error)?
            .into_std();
        let cleanup = UploadCleanup {
            dir: dir.try_clone().map_err(io_error)?,
            name: temp.clone(),
        };
        let mut output = tokio::fs::File::from_std(output);
        let copied: Result<u64, String> = async {
            let mut size = 0;
            while let Some(bytes) = field.chunk().await.map_err(|_| "上传中断")? {
                size += bytes.len() as u64;
                if size > UPLOAD_LIMIT {
                    return Err("单个文件不得超过 500 MiB".into());
                }
                tokio::io::AsyncWriteExt::write_all(&mut output, &bytes)
                    .await
                    .map_err(io_error)?;
            }
            output.sync_all().await.map_err(io_error)?;
            drop(output);
            drop(field);
            Ok(size)
        }
        .await;
        let size = copied?;
        if multipart
            .next_field()
            .await
            .map_err(|_| "上传内容无效")?
            .is_some()
        {
            return Err("每次仅支持上传一个文件".into());
        }
        dir.hard_link(&temp, &dir, &name).map_err(io_error)?;
        drop(cleanup);
        audit(&state, &context, "files.upload.completed", &path).await?;
        Ok(json!({"path":path,"size":size}))
    }
    .await;
    reply(result, &context)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_sizes_are_explicit_and_renames_preserve_conflicting_targets() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        let dir = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
        dir.create_dir("nested").unwrap();
        dir.write("a", b"abc").unwrap();
        dir.write("nested/b", b"hello").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc", root.join("external-link")).unwrap();
        let size = directory_size(dir.try_clone().unwrap()).unwrap();
        assert_eq!(size["bytes"], 8);
        assert_eq!(size["files"], 2);
        assert_eq!(size["directories"], 1);
        rename_entry(&dir, std::ffi::OsStr::new("a"), "renamed", false).unwrap();
        assert_eq!(dir.read("renamed").unwrap(), b"abc");
        assert!(!dir.exists("a"));
        dir.write("a", b"original").unwrap();
        assert!(rename_entry(&dir, std::ffi::OsStr::new("renamed"), "a", false).is_err());
        assert_eq!(dir.read("a").unwrap(), b"original");
        assert_eq!(dir.read("renamed").unwrap(), b"abc");
        dir.create_dir("existing").unwrap();
        assert!(rename_entry(&dir, std::ffi::OsStr::new("nested"), "existing", true).is_err());
        rename_entry(&dir, std::ffi::OsStr::new("nested"), "renamed-dir", true).unwrap();
        assert_eq!(dir.read("renamed-dir/b").unwrap(), b"hello");
        drop(dir);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn deletion_protects_root_but_allows_user_created_entries() {
        assert!(!delete_allowed("/", FsPath::new("."), true));
        assert!(!delete_allowed("/", FsPath::new("etc"), true));
        assert!(delete_allowed("/", FsPath::new("uploaded.txt"), false));
        assert!(delete_allowed("/", FsPath::new("my-folder"), true));
        assert!(delete_allowed("/", FsPath::new("tmp/my-folder"), true));
        assert!(delete_allowed("/data", FsPath::new("etc"), true));
    }
    #[test]
    fn paths_and_names_cannot_escape_root() {
        for path in ["../secret", "/a/../../secret", "C:/Windows", "a\\b"] {
            assert!(Files::path(path).is_err());
        }
        for name in ["..", "a/b", "a\\b", ""] {
            assert!(child("/", name).is_err());
        }
        assert!(Files::path("/var/log").is_ok());
    }
    #[test]
    fn capability_rejects_symlink_escape_and_never_overwrites() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        let dir = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();
        dir.write("kept", b"original").unwrap();
        assert!(
            dir.open_with(
                "kept",
                cap_std::fs::OpenOptions::new().write(true).create_new(true)
            )
            .is_err()
        );
        assert_eq!(dir.read("kept").unwrap(), b"original");
        assert!(dir.open("../outside").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc", root.join("escape")).unwrap();
            assert!(dir.open("escape/passwd").is_err());
        }
        drop(dir);
        std::fs::remove_dir_all(root).unwrap();
    }
}
