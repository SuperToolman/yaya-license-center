use super::*;
use backup::Archive;
fn save(state: &AppState, task: &str, stage: &str) -> Result<(), String> {
    db(state)?
        .execute(
            "UPDATE container_version_tasks SET status='running',stage=? WHERE id=?",
            params![stage, task],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn error_text(e: Failure) -> String {
    e.1.0.message
}
async fn command(args: &[&str]) -> Result<(), String> {
    local_docker::run(args, 120).await.map(|_| ())
}
pub(super) async fn dispatch(state: AppState, id: String, req: Request, headers: HeaderMap) {
    let result = async {
        let user = admin(&headers, &state).await.map_err(error_text)?;
        if user.role != "platform_admin" {
            return Err("管理员会话已失效".into());
        }
        execute(&state, &id, &req, &headers).await
    }
    .await;
    if let Err(e) = result {
        let stage: String = db(&state)
            .ok()
            .and_then(|d| {
                d.query_row(
                    "SELECT stage FROM container_version_tasks WHERE id=?",
                    [&req.task_id],
                    |r| r.get(0),
                )
                .ok()
            })
            .unwrap_or_default();
        let restored = if ["queued", "preparing"].contains(&stage.as_str()) {
            Ok(())
        } else {
            recover_task(&state, &id, &req.task_id).await
        };
        if let Ok(d) = db(&state) {
            let status = if restored.is_ok() {
                "failed"
            } else {
                "attention"
            };
            let re = restored.err();
            let _=d.execute("UPDATE container_version_tasks SET status=?,error=?,recovery_error=?,finished_at=? WHERE id=?",params![status,e,re,Utc::now().timestamp_millis(),req.task_id]);
            let _ = event(
                &d,
                &id,
                &format!("version.{}.{}", req.kind, status),
                "version-executor",
            );
        }
    }
}
async fn execute(
    state: &AppState,
    id: &str,
    req: &Request,
    headers: &HeaderMap,
) -> Result<(), String> {
    save(state, &req.task_id, "preparing")?;
    let (record, v) = target(state, id, &req.expected_container_id).await?;
    let actor: String = db(state)?
        .query_row(
            "SELECT actor_id FROM container_version_tasks WHERE id=?",
            [&req.task_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    backup::exclusive(&v).await?;
    let helper = backup::helper().await?;
    if req.kind == "upgrade" {
        let meta = package(state, req.package_id.as_deref().ok_or("请选择目标版本")?)?;
        resolve_environment(&env_map(&v["Config"]["Env"]), &spec(&meta)?, req)?;
    }
    let target_image = if req.kind == "upgrade" {
        Some(load_package(state, req.package_id.as_deref().ok_or("请选择目标版本")?).await?)
    } else {
        None
    };
    if v["HostConfig"]["Privileged"] == true
        || v["HostConfig"]["Devices"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
        || v["HostConfig"]["VolumesFrom"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    {
        return Err("特权、设备映射或 volumes-from 容器须先改为标准部署配置".into());
    }
    admin(headers, state).await.map_err(error_text)?;
    let (_, rechecked) = target(state, id, &req.expected_container_id).await?;
    if rechecked["State"]["Status"] != v["State"]["Status"]
        || rechecked["State"]["StartedAt"] != v["State"]["StartedAt"]
    {
        return Err("准备期间容器状态发生变化，请刷新后重新提交".into());
    }
    let mut current = snapshot(&record, &v);
    let encrypted: Option<String> = db(state)?
        .query_row(
            "SELECT environment_encrypted FROM platform_instances WHERE instance_id=?",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    current["environmentEncrypted"] = json!(encrypted);
    let revision = Uuid::new_v4().to_string();
    let point = Uuid::new_v4().to_string();
    {
        let encrypted = seal(state, &revision, &current).map_err(error_text)?;
        let d = db(state)?;
        d.execute(
            "INSERT INTO container_revisions VALUES(?,?,?,?,?,?,?,?)",
            params![
                revision,
                id,
                req.expected_container_id,
                v["Image"].as_str().ok_or("镜像无效")?,
                record.package_id,
                encrypted,
                Utc::now().timestamp_millis(),
                actor
            ],
        )
        .map_err(|e| e.to_string())?;
        d.execute("UPDATE container_version_tasks SET source_revision_id=?,recovery_point_id=?,stage='stopping' WHERE id=?",params![revision,point,req.task_id]).map_err(|e|e.to_string())?;
    }
    if v["State"]["Running"] == true {
        command(&["stop", "--time", "60", &req.expected_container_id]).await?;
    }
    if inspect(&req.expected_container_id).await?["State"]["Running"] == true {
        return Err("原容器仍在运行，停止备份".into());
    }
    backup::exclusive(&inspect(&req.expected_container_id).await?).await?;
    save(state, &req.task_id, "backing_up")?;
    db(state)?.execute("INSERT INTO container_recovery_points(id,instance_id,revision_id,status,created_at,actor_id) VALUES(?,?,?,'creating',?,?)",params![point,id,revision,Utc::now().timestamp_millis(),actor]).map_err(|e|e.to_string())?;
    let files = backup::export(state, &req.task_id, &point, &v, &helper).await;
    let files = match files {
        Ok(v) => v,
        Err(e) => {
            let _ = db(state)?.execute(
                "UPDATE container_recovery_points SET status='failed',error=? WHERE id=?",
                params![e, point],
            );
            return Err(e);
        }
    };
    db(state)?
        .execute(
            "UPDATE container_recovery_points SET status='ready',files_json=?,size=? WHERE id=?",
            params![
                serde_json::to_string(&files).map_err(|e| e.to_string())?,
                files.iter().map(|f| f.size as i64).sum::<i64>(),
                point
            ],
        )
        .map_err(|e| e.to_string())?;
    if req.kind == "backup" {
        if v["State"]["Running"] == true {
            command(&["start", &req.expected_container_id]).await?;
        }
        finish(state, id, &req.task_id)?;
        return Ok(());
    }
    let (source_point, mut target_snapshot, source_files, target_package) = if req.kind == "restore"
    {
        let p = req.recovery_point_id.as_deref().ok_or("缺少恢复点")?;
        let d = db(state)?;
        let (rev,raw):(String,String)=d.query_row("SELECT revision_id,files_json FROM container_recovery_points WHERE id=? AND instance_id=? AND status='ready'",params![p,id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|"恢复点无效")?;
        let encrypted: String = d
            .query_row(
                "SELECT snapshot_encrypted FROM container_revisions WHERE id=?",
                [&rev],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let snap = unseal(state, &rev, &encrypted)?;
        let package = snap["packageId"]
            .as_str()
            .ok_or("历史版本缺失")?
            .to_string();
        engine::json(
            "GET",
            &format!(
                "/images/{}/json",
                snap["imageId"].as_str().ok_or("历史镜像缺失")?
            ),
            None,
        )
        .await
        .map_err(|_| "历史镜像已被外部删除，请重新导入历史版本包")?;
        (
            p.to_string(),
            snap,
            serde_json::from_str::<Vec<Archive>>(&raw).map_err(|_| "备份清单损坏")?,
            package,
        )
    } else {
        (
            point.clone(),
            current.clone(),
            files.clone(),
            req.package_id.clone().unwrap(),
        )
    };
    save(state, &req.task_id, "restoring_volumes")?;
    let volume_specs: Vec<(String, String, Option<usize>)> = if req.kind == "upgrade" {
        let source_spec = spec(&package(state, &record.package_id).unwrap_or_default()).ok();
        spec(&package(state, &target_package)?)?
            .volumes
            .into_iter()
            .map(|vol| {
                let source = source_spec
                    .as_ref()
                    .and_then(|s| s.volumes.iter().find(|s| s.key == vol.key))
                    .map(|s| s.destination.as_str())
                    .unwrap_or(&vol.destination);
                let index = source_files.iter().position(|f| f.destination == source);
                (vol.key, vol.destination, index)
            })
            .collect()
    } else {
        source_files
            .iter()
            .enumerate()
            .map(|(i, f)| (format!("volume-{i}"), f.destination.clone(), Some(i)))
            .collect()
    };
    let mut mounts = vec![];
    for (index, (key, destination, source_index)) in volume_specs.iter().enumerate() {
        let name = format!("yaya-{}-{}-{}", id, req.task_id, key);
        let mut resources = resources(state, &req.task_id)?;
        resources["volumes"]
            .as_array_mut()
            .unwrap()
            .push(json!(name));
        persist_resources(state, &req.task_id, &resources)?;
        engine::json("POST","/volumes/create",Some(json!({"Name":name,"Labels":{"com.yaya.instance-id":id,"com.yaya.version-task":req.task_id}}))).await?;
        if let Some(i) = source_index {
            backup::restore(
                state,
                &req.task_id,
                &source_point,
                *i,
                &source_files[*i],
                &name,
                &helper,
            )
            .await?;
        }
        let read_only = if req.kind == "restore" {
            target_snapshot["mounts"]
                .as_array()
                .and_then(|ms| ms.iter().find(|m| m["Destination"] == destination.as_str()))
                .is_some_and(|m| m["RW"] == false)
        } else {
            false
        };
        mounts
            .push(json!({"Type":"volume","Source":name,"Target":destination,"ReadOnly":read_only}));
        let _ = index;
    }
    let create = &mut target_snapshot["create"];
    if req.kind == "upgrade" {
        let meta = package(state, &target_package)?;
        let s = spec(&meta)?;
        let mut old = env_map(&v["Config"]["Env"]);
        if !old.contains_key("YAYA_PLATFORM_ADMIN_MOBILE") || !old.contains_key("YAYA_PLATFORM_INSTANCE_ID") {
            let record = get_instance(&db(state)?, id).map_err(|(_,v)|v.0.message)?;
            environment::bind_customer(&db(state)?, &record.customer_id, id, &mut old).map_err(|(_,v)|v.0.message)?;
        }
        let mut resolved = resolve_environment(&old, &s, req)?;
        if !req.environment.contains_key("APP_PORT") {
            resolved.insert("APP_PORT".into(), s.api_port.to_string());
        }
        resolved.insert(
            "YAYA_LICENSE_PUBLIC_KEY_PATH".into(),
            "/tmp/yaya-license-public.pem".into(),
        );
        create["Env"] = json!(
            resolved
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
        );
        create["Image"] = json!(target_image.ok_or("目标镜像未准备")?);
        let encrypted =
            environment::encrypt(&state.database_path, id, &resolved).map_err(error_text)?;
        target_snapshot["environmentEncrypted"] = json!(encrypted);
        let image = engine::json(
            "GET",
            &format!(
                "/images/{}/json",
                target_snapshot["create"]["Image"].as_str().unwrap()
            ),
            None,
        )
        .await?;
        for key in [
            "Entrypoint",
            "Cmd",
            "WorkingDir",
            "User",
            "Healthcheck",
            "StopSignal",
            "ExposedPorts",
        ] {
            target_snapshot["create"][key] = image["Config"][key].clone();
        }
        target_snapshot["create"]["HostConfig"]["PortBindings"] = json!({format!("{}/tcp",s.web_port):[{"HostIp":"","HostPort":record.web_port.to_string()}],format!("{}/tcp",s.api_port):[{"HostIp":"","HostPort":record.api_port.to_string()}]});
    }
    let create = &mut target_snapshot["create"];
    create["HostConfig"]["Binds"] = Value::Null;
    create["HostConfig"]["Mounts"] = json!(mounts);
    create["HostConfig"]["AutoRemove"] = json!(false);
    create["Labels"]["com.yaya.instance-id"] = json!(id);
    create["Labels"]["com.yaya.version-task"] = json!(req.task_id);
    let declared = engine::json(
        "GET",
        &format!(
            "/images/{}/json",
            create["Image"].as_str().ok_or("目标镜像无效")?
        ),
        None,
    )
    .await?;
    if let Some(volumes) = declared["Config"]["Volumes"].as_object() {
        for destination in volumes.keys() {
            if !mounts.iter().any(|v| v["Target"] == destination.as_str()) {
                return Err(format!(
                    "目标镜像声明持久目录 {destination}，请在部署清单中显式映射数据卷"
                ));
            }
        }
    }
    create["Volumes"] = json!({});
    let source_renamed = format!("yaya-retained-{}", req.task_id);
    let candidate_name = format!("yaya-candidate-{}", req.task_id);
    save(state, &req.task_id, "creating")?;
    let made = engine::json(
        "POST",
        &format!("/containers/create?name={candidate_name}"),
        Some(create.clone()),
    )
    .await?;
    let candidate = made["Id"].as_str().ok_or("新容器 ID 缺失")?.to_string();
    db(state)?
        .execute(
            "UPDATE container_version_tasks SET target_container_id=? WHERE id=?",
            params![candidate, req.task_id],
        )
        .map_err(|e| e.to_string())?;
    // The public key is a control-plane asset, never restored from old license contents.
    let dir = local_docker::Directory(
        std::env::temp_dir().join(format!("yaya-version-key-{}", req.task_id)),
    );
    fs::create_dir(&dir.0).map_err(|e| e.to_string())?;
    let key = dir.0.join("public.pem");
    super::super::public_key::write(&key)?;
    command(&[
        "cp",
        &key.to_string_lossy(),
        &format!("{candidate}:/tmp/yaya-license-public.pem"),
    ])
    .await?;
    save(state, &req.task_id, "checking")?;
    command(&["start", &candidate]).await?;
    let target_spec = spec(&package(state, &target_package)?).ok();
    health_ports(
        &candidate,
        target_spec.as_ref().map_or(8787, |s| s.web_port),
        target_spec.as_ref().map_or(8788, |s| s.api_port),
    )
    .await?;
    save(state, &req.task_id, "switching")?;
    command(&["update", "--restart=no", &req.expected_container_id]).await?;
    command(&["rename", &req.expected_container_id, &source_renamed]).await?;
    command(&["rename", &candidate, &record.container_name]).await?;
    let new_image = target_snapshot["create"]["Image"]
        .as_str()
        .ok_or("目标镜像无效")?;
    let auth =
        authorization::eligible(&db(state)?, &record.customer_id, None).map_err(error_text)?;
    db(state)?.execute("UPDATE platform_instances SET container_id=?,image_id=?,package_id=?,environment_encrypted=?,authorization_id=?,license_id=?,license_sync_status='syncing',runtime_state=NULL WHERE instance_id=? AND container_id=?",params![candidate,new_image,target_package,target_snapshot["environmentEncrypted"].as_str(),auth.0,auth.1.license_id,id,req.expected_container_id]).map_err(|e|e.to_string())?;
    authorization::apply(state, id).await?;
    let new_record = get_instance(&db(state)?, id).map_err(error_text)?;
    let new_v = inspect(&candidate).await?;
    let revision = Uuid::new_v4().to_string();
    let mut final_snapshot = snapshot(&new_record, &new_v);
    final_snapshot["environmentEncrypted"] = target_snapshot["environmentEncrypted"].clone();
    let enc = seal(state, &revision, &final_snapshot).map_err(error_text)?;
    db(state)?
        .execute(
            "INSERT INTO container_revisions VALUES(?,?,?,?,?,?,?,?)",
            params![
                revision,
                id,
                candidate,
                new_image,
                target_package,
                enc,
                Utc::now().timestamp_millis(),
                actor
            ],
        )
        .map_err(|e| e.to_string())?;
    finish(state, id, &req.task_id)
}
fn resources(state: &AppState, task: &str) -> Result<Value, String> {
    let raw: String = db(state)?
        .query_row(
            "SELECT resources_json FROM container_version_tasks WHERE id=?",
            [task],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if !v["volumes"].is_array() {
        v["volumes"] = json!([]);
    }
    Ok(v)
}
fn persist_resources(state: &AppState, task: &str, value: &Value) -> Result<(), String> {
    db(state)?
        .execute(
            "UPDATE container_version_tasks SET resources_json=? WHERE id=?",
            params![value.to_string(), task],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn finish(state: &AppState, id: &str, task: &str) -> Result<(), String> {
    let d = db(state)?;
    d.execute("UPDATE container_version_tasks SET status='succeeded',stage='completed',finished_at=? WHERE id=?",params![Utc::now().timestamp_millis(),task]).map_err(|e|e.to_string())?;
    event(&d, id, "version.succeeded", "version-executor").map_err(error_text)?;
    Ok(())
}
async fn health_ports(container: &str, web: u16, api: u16) -> Result<(), String> {
    for _ in 0..36 {
        let v = inspect(container).await?;
        if v["State"]["Running"] != true {
            return Err("目标容器启动后退出；将恢复原容器".into());
        }
        if command(&["exec",container,"sh","-c",&format!("curl -fsS --max-time 5 http://127.0.0.1:{api}/healthz >/dev/null && curl -fsS --max-time 5 http://127.0.0.1:{web}/login >/dev/null")]).await.is_ok(){return Ok(());}
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    Err("目标 Web/API 健康检查超时".into())
}
async fn load_package(state: &AppState, id: &str) -> Result<String, String> {
    let meta = package(state, id)?;
    let filename = meta["filename"].as_str().ok_or("包文件名缺失")?;
    if FilePath::new(filename).file_name().and_then(|p| p.to_str()) != Some(filename)
        || filename.contains(['/', '\\'])
    {
        return Err("包路径无效".into());
    }
    let path = platform_versions::storage(state).join(filename);
    let dir = local_docker::Directory(
        std::env::temp_dir().join(format!("yaya-version-image-{}", Uuid::new_v4())),
    );
    fs::create_dir(&dir.0).map_err(|e| e.to_string())?;
    let tar = dir.0.join("image.tar");
    let target = tar.clone();
    let expected: String = db(state)?
        .query_row(
            "SELECT sha256 FROM platform_packages WHERE package_id=?",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let manifest = tokio::task::spawn_blocking(move || {
        let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut b = [0u8; 65536];
        loop {
            let n = f.read(&mut b).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&b[..n]);
        }
        if format!("{:x}", hash.finalize()) != expected {
            return Err("平台包摘要不一致".into());
        }
        platform_versions::unpack_image(&path, &target)
    })
    .await
    .map_err(|e| e.to_string())??;
    local_docker::import_image(manifest.image(), &tar).await
}
pub(super) async fn reconcile(state: &AppState, id: &str) -> Result<(), String> {
    let task:String=db(state)?.query_row("SELECT id FROM container_version_tasks WHERE instance_id=? AND status='attention' ORDER BY created_at DESC LIMIT 1",[id],|r|r.get(0)).map_err(|_|"没有待核对版本任务")?;
    if db(state)?.execute("UPDATE container_version_tasks SET status='recovering' WHERE id=? AND status='attention'",[&task]).map_err(|e|e.to_string())?!=1{return Err("已有恢复核对正在执行".into());}
    match recover_task(state, id, &task).await {
        Ok(()) => {
            db(state)?.execute("UPDATE container_version_tasks SET status='interrupted',stage='reconciled',finished_at=?,recovery_error=NULL WHERE id=?",params![Utc::now().timestamp_millis(),task]).map_err(|e|e.to_string())?;
            Ok(())
        }
        Err(e) => {
            let _ = db(state)?.execute(
                "UPDATE container_version_tasks SET status='attention',recovery_error=? WHERE id=?",
                params![e, task],
            );
            Err(e)
        }
    }
}
async fn recover_task(state: &AppState, id: &str, task: &str) -> Result<(), String> {
    let d = db(state)?;
    let (source,revision,candidate):(String,Option<String>,Option<String>)=d.query_row("SELECT source_container_id,source_revision_id,target_container_id FROM container_version_tasks WHERE id=? AND instance_id=?",params![task,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|e.to_string())?;
    let candidate = match candidate {
        Some(v) => Some(v),
        None => {
            let found = engine::json("GET", "/containers/json?all=1", None).await?;
            found
                .as_array()
                .into_iter()
                .flatten()
                .find(|v| {
                    v["Id"] != source
                        && v["Labels"]["com.yaya.version-task"] == task
                        && v["Labels"]["com.yaya.instance-id"] == id
                })
                .and_then(|v| v["Id"].as_str())
                .map(str::to_owned)
        }
    };
    if let Some(candidate) = candidate {
        let v = inspect(&candidate).await?;
        if v["Config"]["Labels"]["com.yaya.version-task"] != task
            || v["Config"]["Labels"]["com.yaya.instance-id"] != id
        {
            return Err("目标容器归属变化，请人工核对".into());
        }
        command(&["update", "--restart=no", &candidate]).await?;
        if v["State"]["Running"] == true {
            command(&["stop", "--time", "30", &candidate]).await?;
        }
        command(&["rename", &candidate, &format!("yaya-failed-{task}")]).await?;
    }
    let Some(revision) = revision else {
        return Ok(());
    };
    let raw: String = d
        .query_row(
            "SELECT snapshot_encrypted FROM container_revisions WHERE id=?",
            [&revision],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let snap = unseal(state, &revision, &raw)?;
    // Interrupted helper containers must not retain volume mounts or archive processes.
    let helpers = engine::json("GET", "/containers/json?all=1", None).await?;
    for c in helpers.as_array().into_iter().flatten() {
        if c["Labels"]["com.yaya.version-helper"]
            .as_str()
            .is_some_and(|n| n.contains(task))
        {
            if let Some(cid) = c["Id"].as_str() {
                let _ = engine::json("DELETE", &format!("/containers/{cid}?force=1"), None).await;
            }
        }
    }
    let v = inspect(&source).await?;
    if v["Config"]["Labels"]["com.yaya.instance-id"] != id {
        return Err("原容器标签已变化，禁止自动恢复".into());
    }
    let name = snap["name"].as_str().ok_or("原容器名缺失")?;
    if v["Name"].as_str().unwrap_or("").trim_start_matches('/') != name {
        command(&["rename", &source, name]).await?;
    }
    let policy = snap["restartPolicy"]["Name"]
        .as_str()
        .unwrap_or("unless-stopped");
    let retries = snap["restartPolicy"]["MaximumRetryCount"]
        .as_u64()
        .unwrap_or(0);
    let policy = if policy == "on-failure" && retries > 0 {
        format!("on-failure:{retries}")
    } else {
        policy.into()
    };
    command(&["update", &format!("--restart={policy}"), &source]).await?;
    if snap["wasRunning"] == true {
        command(&["start", &source]).await?;
        let source_spec =
            spec(&package(state, snap["packageId"].as_str().unwrap_or("")).unwrap_or_default())
                .ok();
        health_ports(
            &source,
            source_spec.as_ref().map_or(8787, |s| s.web_port),
            source_spec.as_ref().map_or(8788, |s| s.api_port),
        )
        .await?;
    }
    d.execute("UPDATE platform_instances SET container_id=?,image_id=?,package_id=?,environment_encrypted=?,license_id=?,license_sync_status='pending',runtime_state=NULL WHERE instance_id=?",params![source,snap["imageId"].as_str(),snap["packageId"].as_str(),snap["environmentEncrypted"].as_str(),snap["licenseId"].as_str(),id]).map_err(|e|e.to_string())?;
    Ok(())
}
