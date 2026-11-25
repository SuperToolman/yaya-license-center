//! Audited, instance-scoped support access. No staff password enters customer containers.
use super::*;
use serde_json::{Value, json};

fn hash(v: &str) -> String {
    format!("{:x}", Sha256::digest(v.as_bytes()))
}
fn random() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/platform-instances/{id}/support-access",
            post(issue).get(history),
        )
        .route(
            "/api/platform-instances/{id}/support-access/revoke",
            post(revoke),
        )
        .route(
            "/api/platform-instances/{id}/support-access/redeem",
            post(redeem),
        )
        .route(
            "/api/platform-instances/{id}/support-access/check",
            post(check),
        )
        .route(
            "/api/platform-instances/{id}/support-access/logout",
            post(logout),
        )
        .route("/api/platform-instances/{id}/sms/send", post(send_sms))
}
#[derive(Deserialize)]
struct Issue {
    reason: String,
}
#[derive(Deserialize)]
struct Credential {
    credential: String,
    method: Option<String>,
    path: Option<String>,
}

fn audit(
    db: &Connection,
    session: &str,
    name: &str,
    method: Option<&str>,
    path: Option<&str>,
) -> Result<(), Failure> {
    db.execute(
        "INSERT INTO platform_support_events VALUES(?,?,?,?,?,?)",
        params![
            Uuid::new_v4().to_string(),
            session,
            name,
            method,
            path,
            Utc::now().timestamp_millis()
        ],
    )
    .map_err(db_error)?;
    Ok(())
}
fn source_valid(db: &Connection, session: &str) -> Result<bool, Failure> {
    let now = Utc::now().timestamp_millis();
    let row:Option<(String,String,String,String)>=db.query_row("SELECT u.password_hash,u.role,u.organization_id,s.credential_fingerprint FROM staff_sessions s JOIN users u ON u.user_id=s.user_id WHERE s.token_hash=? AND s.revoked_at_ms IS NULL AND s.expires_at_ms>? AND s.idle_expires_at_ms>? AND u.status='active' AND u.role='platform_admin'",params![session,now,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(db_error)?;
    Ok(row.is_some_and(|(password, role, org, saved)| {
        crate::staff_sessions::fingerprint(&password, &role, &org) == saved
    }))
}
fn instance_auth(
    db: &Connection,
    crypto: &FilePath,
    id: &str,
    provided: &str,
) -> Result<(), Failure> {
    let r = get_instance(db, id)?;
    if r.status != "running" || r.license_sync_status != "synced" {
        return Err(bad("仅已部署并完成授权的运行中实例支持运维访问"));
    }
    authorization::eligible(db, &r.customer_id, None)?;
    let encrypted: String = db
        .query_row(
            "SELECT environment_encrypted FROM platform_instances WHERE instance_id=?",
            [id],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    let env = environment::decrypt(crypto, id, &encrypted)?;
    if provided.is_empty()
        || env
            .get("BACKEND_INTERNAL_TOKEN")
            .is_none_or(|v| hash(v) != hash(provided))
    {
        return Err(error(StatusCode::UNAUTHORIZED, "实例身份校验失败"));
    }
    Ok(())
}
async fn issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Issue>,
) -> Result<Json<Envelope<Value>>, Failure> {
    let actor = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let reason = input.reason.trim().to_owned();
    if reason.chars().count() < 4 || reason.chars().count() > 500 {
        return Err(bad("请填写 4–500 字的运维访问原因"));
    }
    let source = hash(cookie_value(&headers, "license_center_session").unwrap_or(""));
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let result=work(state,move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        if !source_valid(&tx,&source)? {return Err(error(StatusCode::UNAUTHORIZED,"运营登录已失效"));}
        let r=get_instance(&tx,&id)?;
        if r.status!="running" || r.license_sync_status!="synced" {return Err(bad("请先完成实例部署及授权"));}
        authorization::eligible(&tx,&r.customer_id,None)?;
        let now=Utc::now().timestamp_millis();
        let recent:i64=tx.query_row("SELECT count(*) FROM platform_support_sessions WHERE actor_user_id=? AND created_at>?",params![actor.user_id,now-60_000],|r|r.get(0)).map_err(db_error)?;
        if recent>=10 {return Err(bad("运维登录请求过于频繁，请稍后再试"));}
        let mut target=if r.public_url.is_empty(){
            let mut url=reqwest::Url::parse(&origin).map_err(db_error)?;
            url.set_port(Some(r.web_port)).map_err(|_|bad("无法确定实例访问端口"))?;
            url.set_scheme("http").map_err(|_|bad("实例访问地址无效"))?;
            url
        }else{reqwest::Url::parse(&r.public_url).map_err(db_error)?};
        target.set_path("/operations-login");target.set_query(None);target.set_fragment(None);
        let ticket=random();let key=Uuid::new_v4().to_string();
        tx.execute("INSERT INTO platform_support_sessions(id,instance_id,actor_user_id,source_session_hash,code_hash,reason,created_at,code_expires_at,expires_at) VALUES(?,?,?,?,?,?,?,?,?)",params![key,id,actor.user_id,source,hash(&ticket),reason,now,now+60_000,now+30*60_000]).map_err(db_error)?;
        audit(&tx,&key,"issued",None,None)?;
        target.set_fragment(Some(&format!("code={ticket}")));
        tx.commit().map_err(db_error)?;
        Ok(json!({"url":target.as_str(),"expiresIn":60,"sessionId":key}))
    }).await?;
    Ok(Json(success("运维登录凭证已生成，60 秒内有效", result)))
}
fn internal_token(headers: &HeaderMap) -> String {
    headers
        .get("x-yaya-instance-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .into()
}
async fn redeem(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Credential>,
) -> Result<Json<Envelope<Value>>, Failure> {
    let provided = internal_token(&headers);
    let crypto = state.database_path.clone();
    let result=work(state,move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        instance_auth(&tx,&crypto,&id,&provided)?;
        let now=Utc::now().timestamp_millis();
        let row:Option<(String,String,String,String,i64)>=tx.query_row("SELECT s.id,s.actor_user_id,s.source_session_hash,u.username,s.expires_at FROM platform_support_sessions s JOIN users u ON u.user_id=s.actor_user_id WHERE s.code_hash=? AND s.instance_id=? AND s.connected_at IS NULL AND s.ended_at IS NULL AND s.code_expires_at>?",params![hash(&input.credential),id,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(db_error)?;
        let (key,actor,source,name,expires)=row.ok_or_else(||error(StatusCode::UNAUTHORIZED,"运维登录凭证已使用或已过期"))?;
        if !source_valid(&tx,&source)? {return Err(error(StatusCode::UNAUTHORIZED,"运营账号或登录会话已失效"));}
        let token=random();
        tx.execute("UPDATE platform_support_sessions SET session_hash=?,connected_at=? WHERE id=?",params![hash(&token),now,key]).map_err(db_error)?;
        audit(&tx,&key,"connected",None,None)?;
        tx.commit().map_err(db_error)?;
        Ok(json!({"actorId":actor,"username":name,"session":token,"expiresAt":expires}))
    }).await?;
    Ok(Json(success("运维访问已授权", result)))
}
async fn session_action(
    state: AppState,
    headers: HeaderMap,
    id: String,
    input: Credential,
    end: bool,
) -> Result<Json<Envelope<Value>>, Failure> {
    let provided = internal_token(&headers);
    let crypto = state.database_path.clone();
    let result=work(state,move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        instance_auth(&tx,&crypto,&id,&provided)?;
        let now=Utc::now().timestamp_millis();
        let row:Option<(String,String,String)>=tx.query_row("SELECT id,actor_user_id,source_session_hash FROM platform_support_sessions WHERE session_hash=? AND instance_id=? AND connected_at IS NOT NULL AND ended_at IS NULL AND expires_at>?",params![hash(&input.credential),id,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(db_error)?;
        let (key,actor,source)=row.ok_or_else(||error(StatusCode::UNAUTHORIZED,"运维会话已过期或撤销"))?;
        if !source_valid(&tx,&source)? {return Err(error(StatusCode::UNAUTHORIZED,"运营账号或登录会话已失效"));}
        if end {
            tx.execute("UPDATE platform_support_sessions SET ended_at=? WHERE id=?",params![now,key]).map_err(db_error)?;
            audit(&tx,&key,"disconnected",None,None)?;
        }else if let (Some(method),Some(path))=(input.method.as_deref(),input.path.as_deref()) {
            if method.len()>16 || !path.starts_with('/') || path.len()>500 || path.contains(['?','#','\r','\n']) {return Err(bad("运维审计请求格式无效"));}
            audit(&tx,&key,"request",Some(method),Some(path))?;
        }
        tx.commit().map_err(db_error)?;
        Ok(json!({"actorId":actor,"valid":true}))
    }).await?;
    Ok(Json(success("运维会话已核对", result)))
}
async fn check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Credential>,
) -> Result<Json<Envelope<Value>>, Failure> {
    session_action(state, headers, id, input, false).await
}
async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Credential>,
) -> Result<Json<Envelope<Value>>, Failure> {
    session_action(state, headers, id, input, true).await
}
async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<Value>>, Failure> {
    admin(&headers, &state).await?;
    let result=work(state,move|db|{
        get_instance(db,&id)?;
        let mut stmt=db.prepare("SELECT s.id,u.username,s.reason,s.created_at,s.connected_at,s.expires_at,s.ended_at FROM platform_support_sessions s JOIN users u ON u.user_id=s.actor_user_id WHERE s.instance_id=? ORDER BY s.created_at DESC LIMIT 100").map_err(db_error)?;
        let sessions=stmt.query_map([&id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"username":r.get::<_,String>(1)?,"reason":r.get::<_,String>(2)?,"createdAt":r.get::<_,i64>(3)?,"connectedAt":r.get::<_,Option<i64>>(4)?,"expiresAt":r.get::<_,i64>(5)?,"endedAt":r.get::<_,Option<i64>>(6)?}))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
        let mut stmt=db.prepare("SELECT e.session_id,e.event,e.method,e.path,e.created_at FROM platform_support_events e JOIN platform_support_sessions s ON s.id=e.session_id WHERE s.instance_id=? ORDER BY e.created_at DESC LIMIT 300").map_err(db_error)?;
        let events=stmt.query_map([id],|r|Ok(json!({"sessionId":r.get::<_,String>(0)?,"event":r.get::<_,String>(1)?,"method":r.get::<_,Option<String>>(2)?,"path":r.get::<_,Option<String>>(3)?,"createdAt":r.get::<_,i64>(4)?}))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
        Ok(json!({"sessions":sessions,"events":events}))
    }).await?;
    Ok(Json(success("运维访问记录", result)))
}
async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<Value>>, Failure> {
    let actor = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    work(state,move|db|{get_instance(db,&id)?;db.execute("UPDATE platform_support_sessions SET ended_at=? WHERE instance_id=? AND ended_at IS NULL",params![Utc::now().timestamp_millis(),id]).map_err(db_error)?;event(db,&id,"support.revoked",&actor.user_id)?;Ok(())}).await?;
    Ok(Json(success("该实例运维访问已撤销", json!({}))))
}
#[derive(Deserialize)]
struct Sms {
    mobile: String,
    code: String,
}
async fn send_sms(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Sms>,
) -> Result<Json<Envelope<Value>>, Failure> {
    if !regex::Regex::new(r"^1[3-9][0-9]{9}$")
        .unwrap()
        .is_match(&input.mobile)
        || input.code.len() != 6
        || !input.code.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(bad("短信参数无效"));
    }
    let crypto = state.database_path.clone();
    let token = internal_token(&headers);
    let mobile = input.mobile.clone();
    work(state,move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        instance_auth(&tx,&crypto,&id,&token)?;
        let now=Utc::now().timestamp_millis();
        tx.execute("DELETE FROM platform_instance_sms_attempts WHERE created_at<?",[now-3_600_000]).map_err(db_error)?;
        let (phone_count,last,instance_count):(i64,i64,i64)=tx.query_row("SELECT (SELECT count(*) FROM platform_instance_sms_attempts WHERE mobile=?),(SELECT COALESCE(MAX(created_at),0) FROM platform_instance_sms_attempts WHERE mobile=?),(SELECT count(*) FROM platform_instance_sms_attempts WHERE instance_id=?)",params![mobile,mobile,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(db_error)?;
        if phone_count>=5 || now-last<60_000 || instance_count>=50 {return Err(error(StatusCode::TOO_MANY_REQUESTS,"短信发送频繁，请稍后再试"));}
        tx.execute("INSERT INTO platform_instance_sms_attempts VALUES(?,?,?,?)",params![Uuid::new_v4().to_string(),id,mobile,now]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;Ok(())
    }).await?;
    crate::website_auth::send_customer_instance_sms(&input.mobile, &input.code).await?;
    Ok(Json(success("验证码已发送", json!({}))))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Fixture;
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    async fn internal(
        f: &Fixture,
        id: &str,
        action: &str,
        credential: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let response = router()
            .with_state(f.state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/api/platform-instances/{id}/support-access/{action}"
                    ))
                    .header("content-type", "application/json")
                    .header("x-yaya-instance-token", token)
                    .body(Body::from(
                        json!({"credential":credential,"method":"GET","path":"/api/apps"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap())
    }
    #[tokio::test]
    async fn support_credentials_are_single_use_scoped_revocable_and_audited() {
        let f = Fixture::new("platform_admin");
        let (_, created) = f
            .call(
                "POST",
                "/api/platform-instances",
                Some(super::super::tests::input("Support test")),
                true,
            )
            .await;
        let id = created["data"]["instanceId"].as_str().unwrap();
        let db = Connection::open(&f.state.database_path).unwrap();
        db.execute("UPDATE platform_instances SET deployment_status='running',license_sync_status='synced' WHERE instance_id=?",[id]).unwrap();
        let encrypted: String = db
            .query_row(
                "SELECT environment_encrypted FROM platform_instances WHERE instance_id=?",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        let env = environment::decrypt(&f.state.database_path, id, &encrypted).unwrap();
        assert_eq!(env["YAYA_PLATFORM_ADMIN_MOBILE"], "13800138000");
        assert_eq!(env["YAYA_PLATFORM_INSTANCE_ID"], id);
        let token = &env["BACKEND_INTERNAL_TOKEN"];
        let path = format!("/api/platform-instances/{id}/support-access");
        assert_eq!(
            f.call("POST", &path, Some(json!({"reason":"test access"})), false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        let (status, issued) = f
            .call("POST", &path, Some(json!({"reason":"test access"})), true)
            .await;
        assert_eq!(status, StatusCode::OK, "{issued}");
        let url = reqwest::Url::parse(issued["data"]["url"].as_str().unwrap()).unwrap();
        let code = url.fragment().unwrap().strip_prefix("code=").unwrap();
        assert_eq!(
            internal(&f, id, "redeem", code, "wrong").await.0,
            StatusCode::UNAUTHORIZED
        );
        let (status, session) = internal(&f, id, "redeem", code, token).await;
        assert_eq!(status, StatusCode::OK, "{session}");
        assert_eq!(
            internal(&f, id, "redeem", code, token).await.0,
            StatusCode::UNAUTHORIZED
        );
        let credential = session["data"]["session"].as_str().unwrap();
        assert_eq!(
            internal(&f, id, "check", credential, token).await.0,
            StatusCode::OK
        );
        assert_ne!(
            internal(&f, "another-instance", "check", credential, token)
                .await
                .0,
            StatusCode::OK
        );
        let (_, history) = f.call("GET", &path, None, true).await;
        assert!(
            history["data"]["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["path"] == "/api/apps")
        );
        assert!(!history.to_string().contains(code));
        assert!(!history.to_string().contains(credential));
        let (_, another) = f
            .call(
                "POST",
                &path,
                Some(json!({"reason":"explicit session revocation"})),
                true,
            )
            .await;
        let another_url = reqwest::Url::parse(another["data"]["url"].as_str().unwrap()).unwrap();
        let another_code = another_url
            .fragment()
            .unwrap()
            .strip_prefix("code=")
            .unwrap();
        let (_, connected) = internal(&f, id, "redeem", another_code, token).await;
        let connected_token = connected["data"]["session"].as_str().unwrap();
        assert_eq!(
            internal(&f, id, "logout", connected_token, token).await.0,
            StatusCode::OK
        );
        assert_eq!(
            internal(&f, id, "check", connected_token, token).await.0,
            StatusCode::UNAUTHORIZED
        );
        let (_, expired) = f
            .call(
                "POST",
                &path,
                Some(json!({"reason":"expired ticket test"})),
                true,
            )
            .await;
        let expired_url = reqwest::Url::parse(expired["data"]["url"].as_str().unwrap()).unwrap();
        let expired_code = expired_url
            .fragment()
            .unwrap()
            .strip_prefix("code=")
            .unwrap();
        db.execute(
            "UPDATE platform_support_sessions SET code_expires_at=0 WHERE id=?",
            [expired["data"]["sessionId"].as_str().unwrap()],
        )
        .unwrap();
        assert_eq!(
            internal(&f, id, "redeem", expired_code, token).await.0,
            StatusCode::UNAUTHORIZED
        );
        db.execute("UPDATE users SET role='staff' WHERE user_id='u'", [])
            .unwrap();
        assert_eq!(
            internal(&f, id, "check", credential, token).await.0,
            StatusCode::UNAUTHORIZED
        );
        db.execute(
            "UPDATE users SET role='platform_admin' WHERE user_id='u'",
            [],
        )
        .unwrap();
        // Account change triggers permanently revoke the original staff session.
        assert_eq!(
            internal(&f, id, "check", credential, token).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    #[tokio::test]
    async fn customer_mobile_validation_rejects_overrides_and_invalid_customer_phone() {
        let f = Fixture::new("platform_admin");
        let db = Connection::open(&f.state.database_path).unwrap();
        db.execute(
            "UPDATE customers SET contact_phone='10086' WHERE customer_id='c1'",
            [],
        )
        .unwrap();
        let (status, result) = f
            .call(
                "POST",
                "/api/platform-instances",
                Some(super::super::tests::input("invalid phone")),
                true,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{result}");
        assert!(environment::parse("YAYA_PLATFORM_ADMIN_MOBILE=13900139000").is_err());
        assert!(environment::parse("YAYA_PLATFORM_INSTANCE_ID=someone-else").is_err());
    }
}
