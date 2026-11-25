use super::*;
use serde_json::{Value, json};
fn bad_owned(v:String)->Failure {bad(&v)}
pub(super) fn eligible(
    db: &Connection,
    customer: &str,
    requested: Option<&str>,
) -> Result<(String, LicenseRecord), Failure> {
    let record = crate::licensing::latest(db, customer)
        .map_err(db_error)?
        .ok_or_else(|| bad("客户尚无可用平台授权，请先完成订单回款和授权交付"))?;
    if record.expires_at <= Utc::now().timestamp()
        || !record.modules.iter().any(|m| m == "platform")
    {
        return Err(bad("客户平台授权已过期或不包含平台权益"));
    }
    if requested.is_some_and(|id| id != record.license_id) {
        return Err(error(
            StatusCode::CONFLICT,
            "许可证不属于当前客户或已有新版本，请刷新授权后再部署",
        ));
    }
    let active: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=? AND status='active')",
            [customer],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if !active {
        return Err(bad("客户不存在或已停用"));
    }
    let auth = db
        .query_row(
            "SELECT authorization_id FROM customer_authorizations WHERE customer_id=?",
            [customer],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    Ok((auth, record))
}
pub(super) async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<Vec<Value>>>, Failure> {
    admin(&headers, &state).await?;
    let result=work(state,move|db|{get_instance(db,&id)?;let mut q=db.prepare("SELECT id,license_id,status,error,created_at,finished_at FROM platform_license_syncs WHERE instance_id=? ORDER BY created_at DESC LIMIT 30").map_err(db_error)?;
        q.query_map([id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"licenseId":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"error":r.get::<_,Option<String>>(3)?,"createdAt":r.get::<_,i64>(4)?,"finishedAt":r.get::<_,Option<i64>>(5)?}))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)}).await?;
    Ok(Json(success("授权同步记录", result)))
}
pub(super) async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<Value>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let task = Uuid::new_v4().to_string();
    let key = task.clone();
    let result_key = key.clone();
    let instance = id.clone();
    work(state.clone(),move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        let record=get_instance(&tx,&id)?;super::lifecycle::ensure_idle(&tx,&id).map_err(bad_owned)?;
        if record.container_id.is_none() || record.status=="archived" || record.status=="deploying" {return Err(bad("请先完成实例部署；归档或部署中的实例不可同步授权"));}
        let (auth,license)=eligible(&tx,&record.customer_id,None)?;
        tx.execute("INSERT INTO platform_license_syncs VALUES(?,?,?,?,'running',NULL,?,NULL)",params![key,id,license.license_id,user.user_id,Utc::now().timestamp_millis()]).map_err(|_|error(StatusCode::CONFLICT,"此实例已有授权同步任务，请核对同步记录"))?;
        tx.execute("UPDATE platform_instances SET authorization_id=?,license_id=?,license_sync_status='syncing',license_sync_error=NULL WHERE instance_id=?",params![auth,license.license_id,id]).map_err(db_error)?;
        event(&tx,&id,"license.sync.queued",&user.user_id)?;tx.commit().map_err(db_error)?;Ok(())
    }).await?;
    tokio::spawn(async move {
        let result = apply(&state, &instance).await;
        let _=work(state,move|db|{let tx=db.transaction().map_err(db_error)?;
            let error=result.err();let status=if error.is_some(){"failed"}else{"succeeded"};
            tx.execute("UPDATE platform_license_syncs SET status=?,error=?,finished_at=? WHERE id=?",params![status,error,Utc::now().timestamp_millis(),task]).map_err(db_error)?;
            if error.is_some(){tx.execute("UPDATE platform_instances SET license_sync_status='failed',license_sync_error=? WHERE instance_id=?",params![error,instance]).map_err(db_error)?;}
            event(&tx,&instance,&format!("license.sync.{status}"),"local-docker")?;tx.commit().map_err(db_error)?;Ok(())}).await;
    });
    Ok(Json(success(
        "已提交授权同步",
        json!({"taskId":result_key}),
    )))
}
pub(super) async fn apply(state: &AppState, id: &str) -> Result<(), String> {
    let instance = id.to_owned();
    let (record, license) = work(state.clone(), move |db| {
        let record = get_instance(db, &instance)?;
        let (_, license) = eligible(db, &record.customer_id, record.license_id.as_deref())?;
        Ok((record, license))
    })
    .await
    .map_err(|(_, v)| v.0.message)?;
    let target = record.container_id.clone().ok_or("实例尚未创建容器")?;
    let inspected = crate::ops::local_containers::inspect(&[target.clone()]).await?;
    let container = inspected.first().ok_or("目标容器不存在")?;
    if container["id"] != target
        || container["instanceId"] != id
        || container["state"] != "running"
        || crate::ops::local_containers::protected(container)
    {
        return Err("容器绑定不匹配、受保护或未运行，请核对后重试".into());
    }
    let center = std::env::var("YAYA_PLATFORM_LICENSE_CENTER_URL")
        .unwrap_or_else(|_| "http://host.docker.internal:8779".into());
    let url = reqwest::Url::parse(&center).map_err(|_| "运营平台许可地址无效")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "YAYA_PLATFORM_LICENSE_CENTER_URL 必须为容器可访问的 HTTP(S) 运营平台地址".into(),
        );
    }
    // A private temporary file avoids leaking license material through command arguments or task logs.
    let root = local_docker::Directory(
        std::env::temp_dir().join(format!("yaya-license-{}", Uuid::new_v4())),
    );
    fs::create_dir(&root.0).map_err(|_| "授权临时目录创建失败")?;
    let public_key = root.0.join("public.pem");
    super::public_key::write(&public_key)?;
    super::public_key::repair(&target, &public_key).await?;
    let path = root.0.join("license.json");
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|_| "授权临时文件创建失败")?;
    serde_json::to_writer(&mut file,&json!({"licenseCenterUrl":center.trim_end_matches('/'),"license":license.license,"activatedAt":Utc::now().to_rfc3339(),"expectedLicenseId":license.license_id})).map_err(|_|"授权文件写入失败")?;
    drop(file);
    local_docker::run(
        &[
            "cp",
            &path.to_string_lossy(),
            &format!("{target}:/tmp/yaya-license-settings.json"),
        ],
        15,
    )
    .await
    .map_err(|_| "授权文件传输失败，原数据卷已保留")?;
    // Execute a fixed verifier inside the trusted platform image. No application administrator cookie is needed.
    // The protected settings API deliberately requires a user login and cannot be used for unattended delivery.
    let script=root.0.join("apply-license.cjs");
    fs::write(&script,APPLY_SCRIPT).map_err(|_|"授权校验脚本创建失败")?;
    local_docker::run(&["cp",&script.to_string_lossy(),&format!("{target}:/tmp/yaya-apply-license.cjs")],15).await.map_err(|_|"授权校验脚本传输失败")?;
    let result=local_docker::run(&["exec","--user","0",&target,"node","/tmp/yaya-apply-license.cjs"],30).await;
    let _=local_docker::run(&["exec","--user","0",&target,"rm","-f","/tmp/yaya-apply-license.cjs","/tmp/yaya-license-settings.json"],10).await;
    result.map_err(|_|"授权应用或许可中心确认失败；请确认容器可访问 YAYA_PLATFORM_LICENSE_CENTER_URL、验签公钥和有效期后重试（原数据卷保留）")?;
    let key = id.to_owned();
    work(state.clone(),move|db|{let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        eligible(&tx,&record.customer_id,Some(&license.license_id))?;
        if !crate::licensing::activate(&tx,&license).map_err(db_error)? {return Err(bad("授权版本已变化，请重试同步"));}
        tx.execute("UPDATE licenses SET platform_status='running' WHERE license_id=?",[&license.license_id]).map_err(db_error)?;
        tx.execute("UPDATE platform_instances SET applied_license_id=?,license_sync_status='synced',license_sync_error=NULL,license_synced_at=? WHERE instance_id=? AND license_id=?",params![license.license_id,Utc::now().timestamp_millis(),key,license.license_id]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;Ok(())}).await.map_err(|(_,v)|v.0.message)?;
    Ok(())
}

// No dynamic code, shell interpolation, token logging or privileged HTTP authentication bypass.
const APPLY_SCRIPT:&str=r#"
const fs=require('node:fs');const crypto=require('node:crypto');const path=require('node:path');
(async()=>{
 const source='/tmp/yaya-license-settings.json';const settings=JSON.parse(fs.readFileSync(source,'utf8'));
 const parts=settings.license.split('.');if(parts.length!==3)throw Error('signature');
 const header=JSON.parse(Buffer.from(parts[0],'base64url'));const claims=JSON.parse(Buffer.from(parts[1],'base64url'));
 const key=process.env.YAYA_LICENSE_PUBLIC_KEY_PEM||fs.readFileSync(process.env.YAYA_LICENSE_PUBLIC_KEY_PATH,'utf8');
 if(claims.licenseId!==settings.expectedLicenseId||header.alg!=='RS256'||!crypto.verify('RSA-SHA256',Buffer.from(parts[0]+'.'+parts[1]),key,Buffer.from(parts[2],'base64url'))||claims.exp<=Math.floor(Date.now()/1000)||!claims.modules.includes('platform')||!['yaya-operation-center','yaya-license-center'].includes(claims.iss)||claims.aud!=='yaya-low-code')throw Error('invalid license');
 const base=settings.licenseCenterUrl.replace(/\/$/,'');const headers={Authorization:'Bearer '+settings.license};
 const response=await fetch(base+'/api/licenses/'+encodeURIComponent(claims.licenseId)+'/status',{headers,signal:AbortSignal.timeout(10000)});
 const reply=await response.json();if(!response.ok||reply.data?.valid!==true)throw Error('center rejected');
 if(reply.data.latestLicenseId&&reply.data.latestLicenseId!==claims.licenseId)throw Error('newer version exists');
 const destination=process.env.YAYA_LICENSE_SETTINGS_PATH||'/var/lib/yaya/config/license.json';
 if(destination!=='/var/lib/yaya/config/license.json')throw Error('unexpected settings path');
 fs.mkdirSync(path.dirname(destination),{recursive:true});const stat=fs.statSync(path.dirname(destination));
 fs.writeFileSync(destination+'.pending',JSON.stringify({licenseCenterUrl:settings.licenseCenterUrl,license:settings.license,activatedAt:settings.activatedAt}),{mode:0o600});fs.chmodSync(destination+'.pending',0o600);fs.chownSync(destination+'.pending',stat.uid,stat.gid);fs.renameSync(destination+'.pending',destination);
 const activated=await fetch(base+'/api/licenses/'+encodeURIComponent(claims.licenseId)+'/activate',{method:'POST',headers,signal:AbortSignal.timeout(10000)});
 const confirmation=await activated.json();if(!activated.ok||confirmation.data?.valid!==true)throw Error('activation failed');
 process.stdout.write(claims.licenseId);
})().catch(()=>{process.stderr.write('License verification or center confirmation failed');process.exitCode=1;});
"#;
