use super::*;
use axum::response::{IntoResponse, Response};
fn point(state: &AppState, id: &str, p: &str) -> Result<(String, Vec<backup::Archive>), Failure> {
    checked_id(p)?;
    let d = db(state).map_err(bad_owned)?;
    let (revision,files):(String,String)=d.query_row("SELECT revision_id,files_json FROM container_recovery_points WHERE id=? AND instance_id=? AND status='ready'",params![p,id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|bad("恢复点不存在或尚未完成"))?;
    Ok((revision, serde_json::from_str(&files).map_err(db_error)?))
}
pub(super) async fn verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, p)): Path<(String, String)>,
) -> Result<Json<Envelope<()>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let (_, files) = point(&state, &id, &p)?;
    for (i, a) in files.iter().enumerate() {
        backup::verify(&state, &p, i, a).await.map_err(bad_owned)?;
    }
    event(
        &db(&state).map_err(bad_owned)?,
        &p,
        "recovery.verified",
        &user.user_id,
    )?;
    Ok(Json(success("恢复点完整性与解密校验通过", ())))
}
pub(super) async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, p)): Path<(String, String)>,
) -> Result<Response, Failure> {
    let user = admin(&headers, &state).await?;
    let (revision, files) = point(&state, &id, &p)?;
    let d = db(&state).map_err(bad_owned)?;
    let encrypted: String = d
        .query_row(
            "SELECT snapshot_encrypted FROM container_revisions WHERE id=?",
            [&revision],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    event(&d, &p, "recovery.download", &user.user_id)?;
    let folder = root(&state).join(&p);
    let required = files.iter().map(|f| f.size).sum::<u64>() + 256 * 1024 * 1024;
    if fs2::available_space(&folder).map_err(db_error)? < required {
        return Err(bad("生成下载包的空间不足，请释放备份磁盘空间后重试"));
    }
    let bundle = folder.join(format!("export-{}.zip", Uuid::new_v4()));
    let target = bundle.clone();
    let p1 = p.clone();
    let packed=tokio::task::spawn_blocking(move||->Result<(),String>{
  let out=fs::OpenOptions::new().create_new(true).write(true).open(&target).map_err(|e|e.to_string())?;
  let mut zip=zip::ZipWriter::new(out);let opts=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
  zip.start_file("recovery.json",opts).map_err(|e|e.to_string())?;
  zip.write_all(json!({"format":1,"pointId":p1,"revisionId":revision,"instanceId":id,"snapshotEncrypted":encrypted,"files":files}).to_string().as_bytes()).map_err(|e|e.to_string())?;
  for (index,a) in files.iter().enumerate(){if a.file!=format!("{index}.ygcm"){return Err("备份文件名无效".into());}zip.start_file(&a.file,opts).map_err(|e|e.to_string())?;let mut f=fs::File::open(folder.join(&a.file)).map_err(|e|e.to_string())?;std::io::copy(&mut f,&mut zip).map_err(|e|e.to_string())?;}
  zip.finish().map_err(|e|e.to_string())?;Ok(())
 }).await.map_err(db_error)?;
    if let Err(e) = packed {
        let _ = tokio::fs::remove_file(&bundle).await;
        return Err(bad_owned(e));
    }
    let file = tokio::fs::File::open(&bundle).await.map_err(db_error)?;
    let stream = tokio_util::io::ReaderStream::new(file);
    // Remove export after transfer or disconnect; encrypted source archives remain untouched.
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    use futures_util::StreamExt;
    let guard = Cleanup(bundle);
    let stream = stream.map(move |v| {
        let _ = &guard;
        v
    });
    Ok((
        [
            ("content-type", "application/zip".to_string()),
            (
                "content-disposition",
                format!("attachment; filename=\"recovery-{p}.zip\""),
            ),
            ("cache-control", "no-store".into()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
pub(super) async fn key_export(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let key = environment::key(&state.database_path, false)?;
    event(
        &db(&state).map_err(bad_owned)?,
        "recovery-key",
        "recovery.key.export",
        &user.user_id,
    )?;
    Ok((
        [
            ("content-type", "application/octet-stream"),
            (
                "content-disposition",
                "attachment; filename=\"yaya-recovery-master.key\"",
            ),
            ("cache-control", "no-store"),
        ],
        key,
    )
        .into_response())
}
pub(super) async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, p)): Path<(String, String)>,
) -> Result<Json<Envelope<()>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    checked_id(&p)?;
    {
        let mut d = db(&state).map_err(bad_owned)?;
        let tx = d
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(db_error)?;
        ensure_idle(&tx, &id).map_err(bad_owned)?;
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM container_recovery_points WHERE id=? AND instance_id=?)",params![p,id],|r|r.get(0)).map_err(db_error)?;
        if !exists {
            return Err(bad("恢复点不存在"));
        }
        let latest:Option<String>=tx.query_row("SELECT id FROM container_recovery_points WHERE instance_id=? AND status='ready' ORDER BY created_at DESC LIMIT 1",[&id],|r|r.get(0)).optional().map_err(db_error)?;
        if latest.as_deref() == Some(&p) {
            return Err(bad("最近一个可用恢复点受保护，请先创建新恢复点"));
        }
        // Logical deletion first: never let a partially removed archive remain eligible for restore.
        tx.execute(
            "UPDATE container_recovery_points SET status='deleted' WHERE id=?",
            [&p],
        )
        .map_err(db_error)?;
        event(&tx, &p, "recovery.deleted", &user.user_id)?;
        tx.commit().map_err(db_error)?;
    }
    let folder = root(&state).join(&p);
    if folder.exists() {
        tokio::fs::remove_dir_all(folder)
            .await
            .map_err(|_| bad("已标记删除，但文件清理失败，请检查备份目录权限"))?;
    }
    Ok(Json(success("恢复点已删除，容器与原数据卷保留", ())))
}
