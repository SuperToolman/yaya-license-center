use super::*;
use super::{
    engine,
    local_containers::{controllable, inspect, protected, valid_id},
};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::HeaderMap,
};
use futures_util::SinkExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static CAPACITY: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(8)));
const IDLE_SECONDS: u64 = 600;
fn origin(headers: &HeaderMap) -> bool {
    headers.contains_key("origin") && crate::platform_versions::check_origin(headers).is_ok()
}
pub(super) async fn target(state: &AppState, id: &str) -> Result<Value, String> {
    if !valid_id(id) {
        return Err("容器 ID 无效".into());
    }
    let item = inspect(&[id.into()])
        .await?
        .into_iter()
        .next()
        .ok_or("容器不存在")?;
    if protected(&item) || !controllable(state, &item).await {
        return Err("容器受保护或客户实例绑定无效、已归档、正在部署，禁止连接终端".into());
    }
    if item["state"] != "running" {
        return Err("容器未运行，请先启动后连接终端".into());
    }
    Ok(item)
}
pub(super) async fn upgrade(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    headers: HeaderMap,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    if !origin(&headers) {
        return failure(
            StatusCode::FORBIDDEN,
            "ORIGIN_DENIED",
            "终端连接来源不允许",
            &context.request_id,
        );
    }
    if let Err(e) = target(&state, &id).await {
        return failure(
            StatusCode::FORBIDDEN,
            "TERMINAL_DENIED",
            &e,
            &context.request_id,
        );
    }
    let Ok(permit) = CAPACITY.clone().try_acquire_owned() else {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "TERMINAL_BUSY",
            "终端连接数已达上限",
            &context.request_id,
        );
    };
    let session = uuid::Uuid::new_v4().to_string();
    let (saved, container, actor) = (session.clone(), id.clone(), context.user.user_id.clone());
    let queued=db::blocking(state.database_path.clone(),move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
        if let Ok(instance)=tx.query_row("SELECT instance_id FROM platform_instances WHERE container_id=?",[&container],|r|r.get::<_,String>(0)) {crate::platform_delivery::lifecycle::ensure_idle(&tx,&instance)?;}
        let count:i64=tx.query_row("SELECT count(*) FROM ops_terminal_sessions WHERE actor_user_id=? AND status IN ('connecting','connected')",[&actor],|r|r.get(0)).map_err(|e|e.to_string())?;
        if count>=2{return Err("每位管理员最多同时连接两个终端".into())}
        tx.execute("INSERT INTO ops_terminal_sessions(id,container_id,actor_user_id,created_at_ms,status) VALUES(?,?,?,?,'connecting')",params![saved,container,actor,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())
    }).await;
    if let Err(e) = queued {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "TERMINAL_BUSY",
            &e,
            &context.request_id,
        );
    }
    let (failed_state, failed_session) = (state.clone(), session.clone());
    ws.max_message_size(16 * 1024)
        .max_frame_size(16 * 1024)
        .on_failed_upgrade(move |_| {
            tokio::spawn(async move {
                let _ = record(
                    &failed_state,
                    &failed_session,
                    "failed",
                    "WebSocket 握手失败",
                )
                .await;
            });
        })
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            session_run(socket, state, context, headers, id, session).await;
        })
        .into_response()
}
async fn record(state: &AppState, id: &str, status: &str, reason: &str) -> Result<(), String> {
    let (id, status, reason) = (id.to_owned(), status.to_owned(), reason.to_owned());
    db::blocking(state.database_path.clone(),move|db|{
        let tx=db.transaction().map_err(|e|e.to_string())?;
        let now=chrono::Utc::now().timestamp_millis();
        tx.execute("UPDATE ops_terminal_sessions SET status=?,reason=?,connected_at_ms=CASE WHEN ?='connected' THEN ? ELSE connected_at_ms END,disconnected_at_ms=CASE WHEN ?='connected' THEN NULL ELSE ? END WHERE id=?",params![status,reason,status,now,status,now,id]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) SELECT ?,actor_user_id,?,container_id,200,id,? FROM ops_terminal_sessions WHERE id=?",params![uuid::Uuid::new_v4().to_string(),format!("container.terminal.{status}"),now,id]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())
    }).await
}
async fn send(socket: &mut WebSocket, v: Value) -> Result<(), String> {
    tokio::time::timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(v.to_string().into())),
    )
    .await
    .map_err(|_| "浏览器接收超时")?
    .map_err(|_| "浏览器已断开".into())
}
async fn session_run(
    mut socket: WebSocket,
    state: AppState,
    _context: RequestContext,
    headers: HeaderMap,
    id: String,
    session: String,
) {
    let result=async{
        target(&state,&id).await?;
        let exec=engine::json("POST",&format!("/containers/{id}/exec"),Some(json!({"AttachStdin":true,"AttachStdout":true,"AttachStderr":true,"Tty":true,"Cmd":["/bin/sh","-c",r#"printf '\036YAYA_SHELL_PID:%s\037' "$$"; exec /bin/sh -i"#],"Env":["TERM=xterm-256color"]}))).await?;
        let exec_id=exec["Id"].as_str().filter(|v|valid_id(v)).ok_or("Docker 未返回有效 Exec ID")?;
        let saved_exec=exec_id.to_string();let saved_session=session.clone();
        db::blocking(state.database_path.clone(),move|db|db.execute("UPDATE ops_terminal_sessions SET exec_id=? WHERE id=?",params![saved_exec,saved_session]).map(|_|()).map_err(|e|e.to_string())).await?;
        let response=engine::request("POST",&format!("/exec/{exec_id}/start"),Some(json!({"Detach":false,"Tty":true})),true).await?;
        let upgraded=tokio::time::timeout(Duration::from_secs(10),hyper::upgrade::on(response)).await.map_err(|_|"Docker 终端连接超时")?.map_err(|e|format!("Docker TTY 连接失败：{e}"))?;
        let mut stream=hyper_util::rt::TokioIo::new(upgraded);
        // Read a private PID preamble before forwarding output. PID belongs to the container namespace.
        let mut preamble=Vec::new();
        tokio::time::timeout(Duration::from_secs(10),async {
            loop {let b=stream.read_u8().await.map_err(|e|e.to_string())?;preamble.push(b);if b==0x1f{break}if preamble.len()>128{return Err("Shell PID 握手无效".to_string())}}
            Ok::<_,String>(())
        }).await.map_err(|_|"Shell PID 握手超时")??;
        let pid=String::from_utf8(preamble).map_err(|_|"Shell PID 无效")?.trim_start_matches('\u{1e}').trim_end_matches('\u{1f}').strip_prefix("YAYA_SHELL_PID:").ok_or("Shell PID 握手无效")?.to_string();
        if pid.parse::<u32>().ok().is_none_or(|p|p<=1){return Err("Shell PID 无效".into())}
        let (saved_pid,saved_session)=(pid.clone(),session.clone());
        db::blocking(state.database_path.clone(),move|db|db.execute("UPDATE ops_terminal_sessions SET shell_pid=? WHERE id=?",params![saved_pid,saved_session]).map(|_|()).map_err(|e|e.to_string())).await?;
        record(&state,&session,"connected","").await?;
        send(&mut socket,json!({"type":"connected","sessionId":session,"idleSeconds":IDLE_SECONDS})).await?;
        let created=Instant::now();let mut activity=Instant::now();let mut tick=tokio::time::interval(Duration::from_secs(10));
        let mut bytes=[0u8;8192];
        let outcome=async {
            loop{tokio::select!{
                data=stream.read(&mut bytes)=>{
                    let n=data.map_err(|e|e.to_string())?;
                    if n==0{return Ok::<_,String>("Shell 已退出".to_string())}
                    // Binary preserves UTF-8 sequences split across Docker reads; xterm decodes incrementally.
                    tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Binary(bytes[..n].to_vec().into()))).await.map_err(|_|"终端输出接收超时")?.map_err(|_|"浏览器已断开")?;
                },
                message=socket.recv()=>{
                    match message {
                        Some(Ok(Message::Text(text)))=>{
                            let v:Value=serde_json::from_str(&text).map_err(|_|"终端消息格式无效")?;
                            match v["type"].as_str(){
                                Some("input")=>{let input=v["data"].as_str().ok_or("终端输入无效")?;if input.len()>8192{return Err("单次终端输入过大".into())}activity=Instant::now();tokio::time::timeout(Duration::from_secs(5),stream.write_all(input.as_bytes())).await.map_err(|_|"终端输入超时")?.map_err(|e|e.to_string())?;},
                                Some("resize")=>{let(cols,rows)=dimensions(&v)?;engine::json("POST",&format!("/exec/{exec_id}/resize?h={rows}&w={cols}"),None).await?;},
                                Some("close")=>return Ok("用户断开连接".into()),
                                _=>return Err("不支持的终端消息".into())
                            }
                        },
                        Some(Ok(Message::Ping(v)))=>{socket.send(Message::Pong(v)).await.map_err(|_|"浏览器已断开")?;},
                        Some(Ok(Message::Pong(_)))=>{},
                        _=>return Ok("浏览器已断开".into())
                    }
                },
                _=tick.tick()=>{
                    if idle_expired(activity.elapsed()){return Ok("空闲超过 10 分钟，连接已关闭".into())}
                    if created.elapsed().as_secs()>=3600{return Ok("单次终端会话已达 1 小时，请重新连接".into())}
                    let user=crate::require_authenticated(&headers,&state).await.map_err(|_|"登录会话已失效")?;
                    if user.role!="platform_admin"{return Err("管理员权限已撤销".into())}
                    target(&state,&id).await?;
                    tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Ping(vec![].into()))).await.map_err(|_|"浏览器连接超时")?.map_err(|_|"浏览器已断开")?;
                }
            }}
        }.await;
        // Interrupt the foreground program and request shell exit before closing the attach stream.
        let _=tokio::time::timeout(Duration::from_secs(2),async{stream.write_all(b"\x03exit\r").await?;stream.shutdown().await}).await;
        outcome
    }.await;
    let cleanup_state = state.clone();
    let cleanup_session = session.clone();
    let identity = db::blocking(state.database_path.clone(), move |db| {
        db.query_row(
            "SELECT exec_id,shell_pid FROM ops_terminal_sessions WHERE id=?",
            [cleanup_session],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .map_err(|e| e.to_string())
    })
    .await;
    let cleanup_result = match identity {
        Ok((Some(exec), Some(pid))) => Some(
            cleanup(&id, &exec, &pid)
                .await
                .map(|_| "Shell 已退出".to_string())
                .unwrap_or_else(|e| e),
        ),
        Ok((Some(_), None)) => Some("未能读取 Shell PID，请人工核对容器 Exec".to_string()),
        _ => None,
    };
    let saved_session = session.clone();
    let _ = db::blocking(cleanup_state.database_path, move |db| {
        db.execute(
            "UPDATE ops_terminal_sessions SET cleanup_result=? WHERE id=?",
            params![cleanup_result, saved_session],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
    .await;
    let (status, reason) = match result {
        Ok(reason) => ("closed", reason),
        Err(reason) => ("failed", reason),
    };
    let _ = record(&state, &session, status, &reason).await;
    let _ = send(&mut socket, json!({"type":"closed","reason":reason})).await;
    let _ = socket.close().await;
}
fn idle_expired(elapsed: Duration) -> bool {
    elapsed.as_secs() >= IDLE_SECONDS
}
fn dimensions(v: &Value) -> Result<(u64, u64), String> {
    let (cols, rows) = (
        v["cols"].as_u64().unwrap_or(0),
        v["rows"].as_u64().unwrap_or(0),
    );
    if !(20..=500).contains(&cols) || !(5..=200).contains(&rows) {
        return Err("终端尺寸超出范围".into());
    }
    Ok((cols, rows))
}
pub(super) async fn history(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Response {
    if !valid_id(&id) {
        return failure(
            StatusCode::BAD_REQUEST,
            "INVALID_TARGET",
            "容器 ID 无效",
            &context.request_id,
        );
    }
    match db::blocking(state.database_path.clone(),move|db|{
        let mut q=db.prepare("SELECT t.id,t.actor_user_id,COALESCE(NULLIF(u.display_name,''),u.username),t.created_at_ms,t.connected_at_ms,t.disconnected_at_ms,t.status,t.reason,t.cleanup_result FROM ops_terminal_sessions t LEFT JOIN users u ON u.user_id=t.actor_user_id WHERE container_id=? ORDER BY created_at_ms DESC LIMIT 30").map_err(|e|e.to_string())?;
        q.query_map([id],|r|Ok(json!({"cleanupResult":r.get::<_,Option<String>>(8)?,"sessionId":r.get::<_,String>(0)?,"actorId":r.get::<_,String>(1)?,"actorName":r.get::<_,Option<String>>(2)?,"createdAt":r.get::<_,i64>(3)?,"connectedAt":r.get::<_,Option<i64>>(4)?,"disconnectedAt":r.get::<_,Option<i64>>(5)?,"status":r.get::<_,String>(6)?,"reason":r.get::<_,Option<String>>(7)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }).await {Ok(v)=>ok(v).into_response(),Err(_)=>failure(StatusCode::INTERNAL_SERVER_ERROR,"HISTORY_FAILED","终端连接记录读取失败",&context.request_id)}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_and_origins_are_bounded() {
        assert!(dimensions(&json!({"cols":80,"rows":24})).is_ok());
        assert!(dimensions(&json!({"cols":999999,"rows":0})).is_err());
        assert!(!idle_expired(Duration::from_secs(599)));
        assert!(idle_expired(Duration::from_secs(600)));
        assert!(!origin(&HeaderMap::new()));
        let mut h = HeaderMap::new();
        h.insert("origin", "https://attacker.example".parse().unwrap());
        assert!(!origin(&h));
    }
}

async fn cleanup(container: &str, exec_id: &str, pid: &str) -> Result<(), String> {
    if !valid_id(container) || !valid_id(exec_id) || pid.parse::<u32>().ok().is_none_or(|v| v <= 1)
    {
        return Err("Shell 清理身份无效".into());
    }
    if engine::json("GET", &format!("/exec/{exec_id}/json"), None).await?["Running"] != true {
        return Ok(());
    }
    for signal in ["HUP", "KILL"] {
        let cmd = format!("kill -{signal} \"$1\"");
        let helper=engine::json("POST",&format!("/containers/{container}/exec"),Some(json!({"AttachStdout":false,"AttachStderr":false,"Tty":false,"Cmd":["/bin/sh","-c",cmd,"yaya-terminal-cleanup",pid]}))).await?;
        let helper = helper["Id"].as_str().ok_or("Shell 清理执行失败")?;
        engine::json(
            "POST",
            &format!("/exec/{helper}/start"),
            Some(json!({"Detach":true,"Tty":false})),
        )
        .await?;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if engine::json("GET", &format!("/exec/{exec_id}/json"), None).await?["Running"] != true
            {
                return Ok(());
            }
        }
    }
    Err("终端连接已关闭，但 Shell 未退出，请管理员检查".into())
}

pub(crate) fn recover(path: PathBuf) {
    tokio::spawn(async move {
        loop {
            let records=db::blocking(path.clone(),|db|{
        let mut q=db.prepare("SELECT id,container_id,exec_id,shell_pid FROM ops_terminal_sessions WHERE status IN ('interrupted','closed','failed') AND exec_id IS NOT NULL AND shell_pid IS NOT NULL AND (cleanup_result IS NULL OR cleanup_result!='Shell 已退出') ORDER BY created_at_ms DESC LIMIT 100").map_err(|e|e.to_string())?;
        q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }).await;
            if let Ok(records) = records {
                for (session, container, exec, pid) in records {
                    let result = cleanup(&container, &exec, &pid)
                        .await
                        .map(|_| "Shell 已退出".to_string())
                        .unwrap_or_else(|e| e);
                    let _ = db::blocking(path.clone(), move |db| {
                        db.execute(
                            "UPDATE ops_terminal_sessions SET cleanup_result=? WHERE id=?",
                            params![result, session],
                        )
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                    })
                    .await;
                }
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}
