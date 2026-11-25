use super::*;
use std::{process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::local_docker::command;

async fn output(args: &[&str]) -> Result<String, String> {
    let result = tokio::time::timeout(Duration::from_secs(5), command().args(args).output())
        .await
        .map_err(|_| "Docker 响应超时".to_string())?
        .map_err(|_| "当前环境未安装 Docker CLI".to_string())?;
    if !result.status.success() {
        return Err(format!(
            "Docker {} 执行失败：{}",
            args.first().copied().unwrap_or("操作"),
            diagnostic(&result.stdout, &result.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().into())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Runtime {
    available: bool,
    message: String,
    server_version: Option<String>,
}
async fn probe() -> Runtime {
    match output(&["info", "--format", "{{.OSType}}|{{.ServerVersion}}"]).await {
        Ok(value) if value.starts_with("linux|") => Runtime {
            available: true,
            message: "当前环境 Docker 可用，实例将直接部署到此 Docker".into(),
            server_version: Some(value.trim_start_matches("linux|").into()),
        },
        Ok(_) => Runtime {
            available: false,
            message: "当前 Docker 未运行 Linux 容器，请切换到 Linux 容器环境".into(),
            server_version: None,
        },
        Err(message) => Runtime {
            available: false,
            message,
            server_version: None,
        },
    }
}
pub(super) async fn runtime(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Runtime>>, Failure> {
    admin(&headers, &state).await?;
    Ok(Json(success("当前 Docker 环境", probe().await)))
}

pub(super) fn recover(path: &FilePath) -> Result<(), String> {
    super::lifecycle::recover(path)?;
    let mut db = ops::db::open(path).map_err(|e| e.to_string())?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute("UPDATE platform_instances SET deployment_status='failed' WHERE instance_id IN (SELECT instance_id FROM platform_deployments WHERE status IN ('queued','running'))",[]).map_err(|e|e.to_string())?;
    tx.execute("UPDATE platform_deployments SET status='interrupted',stage='interrupted',error='API 重启导致部署中断，请检查已创建容器后重试',finished_at=? WHERE status IN ('queued','running')",[Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
    tx.execute(
        "UPDATE platform_instances SET runtime_state=NULL WHERE container_id IS NOT NULL",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("UPDATE platform_license_syncs SET status='interrupted',error='API 重启导致授权同步中断，请核对实例后重试',finished_at=? WHERE status='running'",[Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
    tx.execute("UPDATE platform_instances SET license_sync_status='interrupted',license_sync_error='API 重启导致同步中断，请重新同步' WHERE license_sync_status='syncing'",[]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub(super) async fn deploy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<String>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let runtime = probe().await;
    if !runtime.available {
        return Err(error(StatusCode::SERVICE_UNAVAILABLE, runtime.message));
    }
    let deployment_id = Uuid::new_v4().to_string();
    let task_id = deployment_id.clone();
    let instance_id = id.clone();
    let public_key = std::env::var("LICENSE_SIGNING_PUBLIC_KEY_PEM")
        .map_err(|_| bad("当前环境缺少平台许可证验签公钥"))?;
    if !public_key.contains("BEGIN PUBLIC KEY") || public_key.contains("PRIVATE KEY") {
        return Err(bad("平台许可证验签公钥格式错误"));
    }
    work(state.clone(),move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        let record=get_instance(&tx,&id)?;
        authorization::eligible(&tx,&record.customer_id,record.license_id.as_deref())?;
        let supported: bool = tx.query_row("SELECT json_extract(metadata,'$.artifactType')='offline-image' FROM platform_packages WHERE package_id=?",[&record.package_id],|r|r.get::<_,Option<bool>>(0)).map_err(db_error)?.unwrap_or(false);
        if !supported { return Err(bad("此实例使用的版本格式已停用，请编辑实例并选择离线镜像版本")); }
        if !["not_deployed","failed"].contains(&record.status.as_str()) || record.container_id.is_some(){return Err(error(StatusCode::CONFLICT,"此实例已有容器、正在部署或已归档，请先检查现有容器"));}
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM platform_deployments WHERE instance_id=? AND status IN ('queued','running'))",[&id],|r|r.get(0)).map_err(db_error)?;
        if active{return Err(error(StatusCode::CONFLICT,"已有执行中的部署任务"));}
        tx.execute("INSERT INTO platform_deployments(deployment_id,instance_id,status,created_by,created_at) VALUES(?,?,'queued',?,?)",params![task_id,id,user.user_id,Utc::now().timestamp_millis()]).map_err(db_error)?;
        tx.execute("UPDATE platform_instances SET deployment_status='deploying',updated_at=? WHERE instance_id=?",params![Utc::now().timestamp_millis(),id]).map_err(db_error)?;
        event(&tx,&task_id,"deployment.queued",&user.user_id)?;tx.commit().map_err(db_error)?;Ok(())
    }).await?;
    let response_id = deployment_id.clone();
    tokio::spawn(async move {
        let result = execute(&state, &instance_id, &deployment_id, &public_key).await;
        let _=work(state,move|db|{
            let tx=db.transaction().map_err(db_error)?;
            let (task_status,instance_status,stage,err)=match result{Ok(())=>("succeeded","running","completed",None),Err(message)=>("failed","failed","failed",Some(message))};
            tx.execute("UPDATE platform_deployments SET status=?,stage=CASE WHEN ?='failed' THEN stage ELSE ? END,error=?,finished_at=? WHERE deployment_id=?",params![task_status,task_status,stage,err,Utc::now().timestamp_millis(),deployment_id]).map_err(db_error)?;
            if err.is_some() { tx.execute("UPDATE platform_instances SET license_sync_status='failed',license_sync_error=? WHERE instance_id=? AND EXISTS(SELECT 1 FROM platform_deployments WHERE deployment_id=? AND stage='licensing')",params![err,instance_id,deployment_id]).map_err(db_error)?; }
            tx.execute("UPDATE platform_instances SET deployment_status=?,updated_at=? WHERE instance_id=?",params![instance_status,Utc::now().timestamp_millis(),instance_id]).map_err(db_error)?;
            event(&tx,&deployment_id,&format!("deployment.{task_status}"),"local-docker")?;tx.commit().map_err(db_error)?;Ok(())
        }).await;
    });
    Ok(Json(success("已开始在当前 Docker 环境部署", response_id)))
}
pub(super) async fn deployments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<Vec<serde_json::Value>>>, Failure> {
    admin(&headers, &state).await?;
    let result=work(state,move|db|{get_instance(db,&id)?;let mut q=db.prepare("SELECT deployment_id,status,stage,log_tail,error,created_at,finished_at FROM platform_deployments WHERE instance_id=? ORDER BY created_at DESC LIMIT 20").map_err(db_error)?;
        q.query_map([id],|r|Ok(serde_json::json!({"deploymentId":r.get::<_,String>(0)?,"status":r.get::<_,String>(1)?,"stage":r.get::<_,String>(2)?,"logTail":r.get::<_,String>(3)?,"error":r.get::<_,Option<String>>(4)?,"createdAt":r.get::<_,i64>(5)?,"finishedAt":r.get::<_,Option<i64>>(6)?}))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }).await?;
    Ok(Json(success("部署记录", result)))
}
async fn stage(state: &AppState, id: &str, value: &str, message: &str) -> Result<(), String> {
    let id = id.to_string();
    let value = value.to_string();
    let message = format!("{}\n", message);
    work(state.clone(),move|db|{db.execute("UPDATE platform_deployments SET status='running',stage=?,log_tail=substr(log_tail || ?,-24000) WHERE deployment_id=?",params![value,message,id]).map_err(db_error)?;Ok(())}).await.map_err(|_|"部署记录保存失败".to_string())
}
pub(super) struct Directory(pub(super) PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(crate) fn diagnostic(stdout: &[u8], stderr: &[u8]) -> String {
    let raw = if stderr.is_empty() { stdout } else { stderr };
    let text = String::from_utf8_lossy(raw);
    let ansi = regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").unwrap();
    let text = ansi.replace_all(&text, "");
    let credentials = regex::Regex::new(
        r"(?i)\b(password|token|secret|api[_-]?key|authorization|credential)(\s*[=:]\s*)([^\s,;]+)",
    )
    .unwrap();
    let text = credentials.replace_all(&text, "$1$2[redacted]");
    let urls = regex::Regex::new(r"(?i)(https?://)[^\s/@]+:[^\s/@]+@").unwrap();
    let text = urls.replace_all(&text, "$1[redacted]@");
    let detail: String = text
        .trim()
        .chars()
        .rev()
        .take(2400)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if detail.is_empty() {
        "Docker 未返回错误详情".into()
    } else {
        detail
    }
}

pub(super) async fn run(args: &[&str], seconds: u64) -> Result<String, String> {
    run_command(command(), args, seconds).await
}

async fn run_command(mut cmd: Command, args: &[&str], seconds: u64) -> Result<String, String> {
    let mut child = cmd
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "无法启动 Docker CLI".to_string())?;
    let mut stderr = child.stderr.take().ok_or("无法读取 Docker 错误输出")?;
    // Drain continuously so long builds cannot fill the pipe or grow memory without bound.
    let reader = async move {
        let mut tail = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stderr
                .read(&mut chunk)
                .await
                .map_err(|_| "读取 Docker 输出失败")?;
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&chunk[..n]);
            if tail.len() > 16384 {
                tail.drain(..tail.len() - 16384);
            }
        }
        Ok::<_, String>(tail)
    };
    let (status, stderr) = tokio::time::timeout(Duration::from_secs(seconds), async {
        let (status, tail) = tokio::join!(child.wait(), reader);
        Ok::<_, String>((status.map_err(|_| "Docker 执行失败")?, tail?))
    })
    .await
    .map_err(|_| "Docker 操作超时，请检查本机 Docker 状态".to_string())??;
    if !status.success() {
        let operation = if args.first() == Some(&"image") {
            format!("image {}", args.get(1).copied().unwrap_or("operation"))
        } else {
            args.first().copied().unwrap_or("operation").to_string()
        };
        return Err(format!(
            "Docker {operation} 失败（退出码 {}）：{}",
            status.code().unwrap_or(-1),
            diagnostic(&[], &stderr)
        ));
    }
    Ok(String::new())
}
pub(super) async fn import_image(
    expected: &platform_versions::Image,
    image_tar: &FilePath,
) -> Result<String, String> {
    let archive_path = image_tar.to_owned();
    let image = expected.clone();
    let config_id = tokio::task::spawn_blocking(move || archive_identity(&image, &archive_path))
        .await.map_err(|_| "读取镜像归档身份任务失败")??;
    let daemon_arch = output(&["info", "--format", "{{.Architecture}}"]).await?;
    let normalized = match daemon_arch.as_str() {
        "x86_64" | "amd64" => "linux/amd64",
        "aarch64" | "arm64" => "linux/arm64",
        _ => return Err(format!("不支持当前 Docker 架构：{daemon_arch}")),
    };
    if expected.platform != normalized {
        return Err(format!(
            "镜像架构 {} 与当前 Docker {} 不匹配，请重新打包",
            expected.platform, normalized
        ));
    }
    run(
        &["image", "load", "--input", &image_tar.to_string_lossy()],
        1800,
    )
    .await?;
    resolve_imported_identity(expected, &config_id, |id| async move {
        output(&["image", "inspect", "--format", "{{.Id}}|{{.Os}}/{{.Architecture}}", &id]).await
    }).await
}

async fn resolve_imported_identity<F, Fut>(expected: &platform_versions::Image, config_id: &String, mut inspect: F) -> Result<String, String>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    // Containerd reports the OCI index ID; classic Docker reports the config ID.
    // Both must come from the checksummed archive. Never fall back to a mutable tag.
    let mut errors = Vec::new();
    for id in [&expected.id, &config_id] {
        match inspect(id.clone()).await {
            Ok(identity) if identity == format!("{id}|{}", expected.platform) => return Ok(id.clone()),
            Ok(_) => return Err("导入镜像 ID 或架构与清单不一致，部署已停止".into()),
            Err(error) => errors.push(error),
        }
    }
    Err(format!("镜像已导入，但未找到归档对应的镜像（索引 {}，配置 {}）：{}", expected.id, config_id, errors.join("；")))
}

mod archive_identity;
use archive_identity::archive_identity;
async fn execute(state: &AppState, id: &str, task: &str, _public_key: &str) -> Result<(), String> {
    stage(state, task, "validating", "正在验证实例配置与版本包").await?;
    let instance_id = id.to_string();
    let db_path = state.database_path.clone();
    let (record, meta, sha, vars) = work(state.clone(), move |db| {
        let record = get_instance(db, &instance_id)?;
        let (meta, sha): (String, String) = db
            .query_row(
                "SELECT metadata,sha256 FROM platform_packages WHERE package_id=?",
                [&record.package_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(db_error)?;
        let encrypted: Option<String> = db
            .query_row(
                "SELECT environment_encrypted FROM platform_instances WHERE instance_id=?",
                [&instance_id],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        let mut vars = environment::decrypt(
            &db_path,
            &instance_id,
            &encrypted.ok_or_else(|| bad("请先编辑并保存完整实例环境配置"))?,
        )?;
        if !vars.contains_key("YAYA_PLATFORM_ADMIN_MOBILE") || !vars.contains_key("YAYA_PLATFORM_INSTANCE_ID") {
            environment::bind_customer(db, &record.customer_id, &instance_id, &mut vars)?;
            let encrypted = environment::encrypt(&db_path, &instance_id, &vars)?;
            db.execute("UPDATE platform_instances SET environment_encrypted=? WHERE instance_id=?",params![encrypted,instance_id]).map_err(db_error)?;
        }
        Ok((record, meta, sha, vars))
    })
    .await
    .map_err(|_| "实例或加密环境变量读取失败".to_string())?;
    let metadata: serde_json::Value =
        serde_json::from_str(&meta).map_err(|_| "版本包元数据损坏")?;
    let filename = metadata["filename"].as_str().ok_or("缺少包文件名")?;
    if FilePath::new(filename).file_name().and_then(|v| v.to_str()) != Some(filename)
        || filename.contains(['/', '\\'])
    {
        return Err("包文件名不合法".into());
    }
    let archive = platform_versions::storage(state).join(filename);
    let root = Directory(std::env::temp_dir().join(format!("yaya-deploy-{task}")));
    fs::create_dir(&root.0).map_err(|_| "创建部署临时目录失败")?;
    let image_tar = root.0.join("image.tar");
    let target = image_tar.clone();
    let manifest =
        tokio::task::spawn_blocking(move || -> Result<platform_versions::Manifest, String> {
            let mut file = fs::File::open(&archive).map_err(|_| "平台包文件缺失")?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 65536];
            loop {
                let n = file.read(&mut buffer).map_err(|_| "读取平台包失败")?;
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
            }
            if format!("{:x}", hash.finalize()) != sha {
                return Err("平台包 SHA-256 与入库记录不一致".into());
            }
            let manifest = platform_versions::unpack_image(&archive, &target)?;
            Ok(manifest)
        })
        .await
        .map_err(|_| "版本包校验任务失败")??;
    stage(
        state,
        task,
        "loading",
        "正在从离线包导入完整镜像，无需连接镜像仓库",
    )
    .await?;
    let image_id = import_image(manifest.image(), &image_tar).await?;
    let deployment_spec = metadata.get("deployment").cloned().filter(|v| !v.is_null()).map(serde_json::from_value::<super::lifecycle::DeploymentSpec>).transpose().map_err(|_| "部署清单无效")?;
    let mut variables = vars;
    variables.insert(
        "YAYA_LICENSE_PUBLIC_KEY_PATH".into(),
        "/tmp/yaya-license-public.pem".into(),
    );
    if let Some(spec)=&deployment_spec {
        let mut effective=manifest.image().environment.as_ref().map(|v|v.iter().filter_map(|v|v.split_once('=').map(|(k,v)|(k.to_owned(),v.to_owned()))).collect::<BTreeMap<_,_>>()).unwrap_or_default();
        for e in &spec.environment {if let Some(value)=&e.default {effective.entry(e.name.clone()).or_insert(value.clone());}}
        effective.extend(variables.clone());
        for e in &spec.environment {if e.required && effective.get(&e.name).is_none_or(|v|v.is_empty()){return Err(format!("请填写目标版本必填环境变量 {}",e.name));}}
        for e in &spec.environment {if !variables.contains_key(&e.name) {if let Some(value)=&e.default {variables.insert(e.name.clone(),value.clone());}}}
    }
    let env_path = root.0.join("instance.env");
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut env_file = options
        .open(&env_path)
        .map_err(|_| "创建临时环境文件失败")?;
    for (key, value) in variables {
        writeln!(env_file, "{key}={value}").map_err(|_| "环境文件写入失败")?;
    }
    drop(env_file);
    let key_path = root.0.join("license-public.pem");
    super::public_key::write(&key_path)?;
    stage(state, task, "creating", "正在创建平台容器及独立持久数据卷").await?;
    // No automatic removal or overwrite of containers with an existing name.
    let db_volume = format!("yaya-{id}-postgres");
    let state_volume = format!("yaya-{id}-state");
    let label = format!("com.yaya.instance-id={id}");
    networks::verify(&record.network_mode, &record.network_name).await?;
    let web = format!("{}:{}", record.web_port, deployment_spec.as_ref().map_or(8787,|s|s.web_port));
    let api = format!("{}:{}", record.api_port, deployment_spec.as_ref().map_or(8788,|s|s.api_port));
    let volumes = deployment_spec.as_ref().map(|s|s.volumes.clone()).unwrap_or_else(||vec![super::lifecycle::VolumeSpec{key:"database".into(),destination:"/var/lib/postgresql".into()},super::lifecycle::VolumeSpec{key:"platform-state".into(),destination:"/var/lib/yaya".into()}]);
    let mut args=vec!["create".to_string(),"--pull".into(),"never".into(),"--name".into(),record.container_name.clone(),"--label".into(),label,"--restart".into(),"unless-stopped".into(),"--network".into(),record.network_name.clone(),"--add-host".into(),"host.docker.internal:host-gateway".into(),"--env-file".into(),env_path.to_string_lossy().into_owned(),"-p".into(),web,"-p".into(),api];
    for volume in volumes {
        let name=match volume.key.as_str(){"database"=>db_volume.clone(),"platform-state"=>state_volume.clone(),_=>format!("yaya-{id}-{}",volume.key)};
        args.extend(["--mount".into(),format!("type=volume,source={name},target={}",volume.destination)]);
    }
    args.push(image_id.clone());
    run(&args.iter().map(String::as_str).collect::<Vec<_>>(),120).await?;
    let container_id = output(&["inspect", "--format", "{{.Id}}", &record.container_name]).await?;
    let saved_id = id.to_string();
    let saved_container = container_id.clone();
    let saved_image = image_id.clone();
    work(state.clone(), move |db| {
        db.execute(
            "UPDATE platform_instances SET container_id=?,image_id=? WHERE instance_id=?",
            params![saved_container, saved_image, saved_id],
        )
        .map_err(db_error)?;
        Ok(())
    })
    .await
    .map_err(|_| "容器已创建但记录保存失败，请检查 Docker 容器".to_string())?;
    // docker cp works for stopped containers and Docker Desktop; no daemon-side bind paths.
    run(
        &[
            "cp",
            &key_path.to_string_lossy(),
            &format!("{container_id}:/tmp/yaya-license-public.pem"),
        ],
        30,
    )
    .await?;
    run(&["start", &container_id], 120).await?;
    stage(
        state,
        task,
        "checking",
        "容器已启动，正在检查 Web 与 API 健康状态",
    )
    .await?;
    for _ in 0..60 {
        if output(&["inspect", "--format", "{{.State.Running}}", &container_id]).await? != "true" {
            return Err("平台容器启动后退出，请检查容器日志；容器和数据卷已保留".into());
        }
        if run(&["exec",&container_id,"sh","-c",&format!("curl -fsS --max-time 5 http://127.0.0.1:{}/healthz >/dev/null && curl -fsS --max-time 5 http://127.0.0.1:{}/login >/dev/null",deployment_spec.as_ref().map_or(8788,|s|s.api_port),deployment_spec.as_ref().map_or(8787,|s|s.web_port))],15).await.is_ok(){stage(state,task,"licensing","正在应用客户许可证并核对授权状态").await?;authorization::apply(state,id).await?;super::lifecycle::capture_initial(state,id).await?;stage(state,task,"completed","Docker 部署成功，Web/API 健康检查通过").await?;return Ok(());}
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Err("健康检查超时；已保留容器和数据卷，请检查平台容器日志".into())
}

#[cfg(test)]
mod tests {
    use super::diagnostic;

    #[tokio::test]
    async fn actual_offline_package_loads_and_matches_image_identity_if_provided() {
        let Some(path) = std::env::var_os("YAYA_TEST_PLATFORM_PACKAGE") else {
            return;
        };
        let root = super::Directory(
            std::env::temp_dir().join(format!("offline-import-test-{}", super::Uuid::new_v4())),
        );
        std::fs::create_dir(&root.0).unwrap();
        let tar = root.0.join("image.tar");
        let manifest =
            super::platform_versions::unpack_image(std::path::Path::new(&path), &tar).unwrap();
        let id = super::import_image(manifest.image(), &tar)
            .await
            .expect("offline docker load and image verification");
        let config_id = super::archive_identity(manifest.image(), &tar).unwrap();
        assert!(id == manifest.image().id || id == config_id);
    }

    #[test]
    fn docker_diagnostic_keeps_actionable_detail_and_redacts_credentials() {
        let detail = diagnostic(
            b"",
            b"ERROR: request failed: token=super-secret connection timed out",
        );
        assert!(detail.contains("connection timed out"));
        assert!(detail.contains("token=[redacted]"));
        assert!(!detail.contains("super-secret"));
    }

    #[test]
    fn docker_diagnostic_is_bounded() {
        let detail = diagnostic(&[], &vec![b'x'; 4000]);
        assert_eq!(detail.chars().count(), 2400);
    }

    #[tokio::test]
    async fn failing_process_returns_exit_code_and_captured_stderr() {
        #[cfg(windows)]
        let (cmd, args) = (
            super::Command::new("cmd.exe"),
            vec![
                "/C",
                "echo token=sensitive connection timed out 1>&2 & exit /b 9",
            ],
        );
        #[cfg(not(windows))]
        let (cmd, args) = (
            super::Command::new("sh"),
            vec![
                "-c",
                "echo 'token=sensitive connection timed out' >&2; exit 9",
            ],
        );
        let error = super::run_command(cmd, &args, 5).await.unwrap_err();
        assert!(error.contains("退出码 9"));
        assert!(error.contains("connection timed out"));
        assert!(!error.contains("sensitive"));
    }
}
