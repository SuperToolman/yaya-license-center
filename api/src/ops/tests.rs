use super::*;
use crate::{LoginRequest, staff_sessions};
use argon2::Argon2;
use axum::http::Request;
use http_body_util::BodyExt;
use jsonwebtoken::{DecodingKey, EncodingKey};
use password_hash::{PasswordHasher, SaltString};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tower::ServiceExt;

struct Fixture {
    state: AppState,
    token: String,
    path: PathBuf,
}
impl Fixture {
    fn new(role: &str, enabled: bool) -> Self {
        let path =
            std::env::temp_dir().join(format!("yaya-ops-api-{}.sqlite3", uuid::Uuid::new_v4()));
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE users(user_id TEXT PRIMARY KEY,username TEXT,password_hash TEXT,role TEXT,organization_id TEXT,status TEXT);").unwrap();
        let salt = SaltString::encode_b64(b"test-fixture-salt-sixteen").unwrap();
        let password = Argon2::default()
            .hash_password(b"password", &salt)
            .unwrap()
            .to_string();
        connection
            .execute(
                "INSERT INTO users VALUES('u1','admin',?,?,'platform','active')",
                params![password, role],
            )
            .unwrap();
        initialize(&path).unwrap();
        let (token, _) = staff_sessions::login(
            &connection,
            LoginRequest {
                username: "admin".into(),
                password: "password".into(),
            },
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
        let state = AppState {
            signing_key: Arc::new(EncodingKey::from_secret(b"test")),
            verification_key: Arc::new(DecodingKey::from_secret(b"test")),
            database_path: path.clone(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            ops: Arc::new(OpsState::new(
                enabled,
                PathBuf::from("/nonexistent/yaya-agent.sock"),
            )),
        };
        Self { state, token, path }
    }
    async fn request(&self, path: &str, authenticate: bool) -> (StatusCode, serde_json::Value) {
        self.send("GET", path, authenticate, Some("http://localhost:8778"), None).await
    }
    async fn send(&self, method: &str, path: &str, authenticate: bool, origin: Option<&str>, body: Option<serde_json::Value>) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder().method(method).uri(path).header("content-type", "application/json");
        if let Some(origin)=origin {builder=builder.header("origin",origin);}
        if authenticate {
            builder = builder.header("cookie", format!("license_center_session={}", self.token));
        }
        let response = router(self.state.clone())
            .with_state(self.state.clone())
            .oneshot(builder.body(body.map(|v|Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for path in [
            self.path.clone(),
            PathBuf::from(format!("{}-wal", self.path.display())),
            PathBuf::from(format!("{}-shm", self.path.display())),
        ] {
            let _ = std::fs::remove_file(path);
        }
    }
}

use rusqlite::Connection;

#[tokio::test]
async fn files_require_admin_origin_and_preserve_files_on_conflict() {
    use serde_json::json;
    let admin = Fixture::new("platform_admin", false);
    let finance = Fixture::new("platform_finance", false);
    for path in ["/api/ops/local/files", "/api/ops/local/files/preview", "/api/ops/local/files/download", "/api/ops/local/files/size", "/api/ops/local/files/properties"] {
        assert_eq!(admin.request(path, false).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(finance.request(path, true).await.0, StatusCode::FORBIDDEN);
    }
    let dir_name = format!(".files-test-{}", uuid::Uuid::new_v4());
    let absolute = std::env::current_dir().unwrap().join(&dir_name);
    std::fs::create_dir(&absolute).unwrap();
    // Never use environment mutation: parallel tests keep their own real directories.
    let path = if cfg!(windows) { format!("/{dir_name}") } else { absolute.to_string_lossy().into_owned() };
    let create = json!({"action":"create","path":path,"name":"kept.txt"});
    assert_eq!(admin.send("POST","/api/ops/local/files/mutate",true,None,Some(create.clone())).await.0,StatusCode::FORBIDDEN);
    assert_eq!(admin.send("POST","/api/ops/local/files/mutate",true,Some("https://evil.test"),Some(create.clone())).await.0,StatusCode::FORBIDDEN);
    assert_eq!(admin.send("POST","/api/ops/local/files/mutate",true,Some("http://localhost:8778"),Some(create.clone())).await.0,StatusCode::OK);
    std::fs::write(absolute.join("kept.txt"), b"original text").unwrap();
    assert_eq!(admin.send("POST","/api/ops/local/files/mutate",true,Some("http://localhost:8778"),Some(create)).await.0,StatusCode::BAD_REQUEST);
    let (status, preview) = admin.request(&format!("/api/ops/local/files/preview?path={path}/kept.txt"),true).await;
    assert_eq!(status,StatusCode::OK);assert_eq!(preview["data"]["text"],"original text");
    let (_, list) = admin.request(&format!("/api/ops/local/files?path={path}"),true).await;
    let revision=list["data"]["items"][0]["revision"].as_str().unwrap().to_string();
    // A failed multipart upload must never overwrite an existing file.
    let boundary="files-test-boundary";
    let body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"kept.txt\"\r\nContent-Type: application/octet-stream\r\n\r\nreplacement\r\n--{boundary}--\r\n");
    let request=Request::builder().method("POST").uri(format!("/api/ops/local/files/upload?path={path}")).header("origin","http://localhost:8778").header("cookie",format!("license_center_session={}",admin.token)).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(Body::from(body.clone())).unwrap();
    let response=router(admin.state.clone()).with_state(admin.state.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(),StatusCode::BAD_REQUEST);
    assert_eq!(std::fs::read(absolute.join("kept.txt")).unwrap(),b"original text");
    assert_eq!(std::fs::read_dir(&absolute).unwrap().count(),1);
    let body=body.replace("kept.txt","uploaded.txt");
    let request=Request::builder().method("POST").uri(format!("/api/ops/local/files/upload?path={path}")).header("origin","http://localhost:8778").header("cookie",format!("license_center_session={}",admin.token)).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(Body::from(body)).unwrap();
    let response=router(admin.state.clone()).with_state(admin.state.clone()).oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
    assert_eq!(std::fs::read(absolute.join("uploaded.txt")).unwrap(),b"replacement");
    let (status, checked)=admin.request(&format!("/api/ops/local/files/download?path={path}/uploaded.txt&check=true"),true).await;
    assert_eq!(status,StatusCode::OK);assert_eq!(checked["data"]["readable"],true);
    assert_eq!(admin.request(&format!("/api/ops/local/files/download?path={path}/missing.txt&check=true"),true).await.0,StatusCode::BAD_REQUEST);
    let request=Request::builder().uri(format!("/api/ops/local/files/download?path={path}/uploaded.txt")).header("cookie",format!("license_center_session={}",admin.token)).body(Body::empty()).unwrap();
    let response=router(admin.state.clone()).with_state(admin.state.clone()).oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert!(response.headers()["content-disposition"].to_str().unwrap().contains("attachment"));
    assert_eq!(response.into_body().collect().await.unwrap().to_bytes(),"replacement");
    let (status, size)=admin.request(&format!("/api/ops/local/files/size?path={path}"),true).await;
    assert_eq!(status,StatusCode::OK);assert_eq!(size["data"]["bytes"],24);
    let (status, properties)=admin.request(&format!("/api/ops/local/files/properties?path={path}/uploaded.txt"),true).await;
    assert_eq!(status,StatusCode::OK);assert_eq!(properties["data"]["name"],"uploaded.txt");assert_eq!(properties["data"]["size"],11);
    let (_, listing)=admin.request(&format!("/api/ops/local/files?path={path}"),true).await;
    let uploaded=listing["data"]["items"].as_array().unwrap().iter().find(|i|i["name"]=="uploaded.txt").unwrap();
    let rename=json!({"action":"rename","path":format!("{path}/uploaded.txt"),"name":"kept.txt","expectedRevision":uploaded["revision"]});
    assert_eq!(admin.send("POST","/api/ops/local/files/mutate",true,Some("http://localhost:8778"),Some(rename)).await.0,StatusCode::BAD_REQUEST);
    let rename=json!({"action":"rename","path":format!("{path}/uploaded.txt"),"name":"z-renamed.txt","expectedRevision":uploaded["revision"]});
    assert_eq!(admin.send("POST","/api/ops/local/files/mutate",true,Some("http://localhost:8778"),Some(rename.clone())).await.0,StatusCode::OK);
    assert!(absolute.join("z-renamed.txt").exists());assert!(!absolute.join("uploaded.txt").exists());
    // A stale confirmation must not remove modified data.
    std::fs::write(absolute.join("kept.txt"),b"new content, different size").unwrap();
    let delete=json!({"action":"delete","path":format!("{path}/kept.txt"),"expectedRevision":revision});
    assert_eq!(admin.send("POST","/api/ops/local/files/mutate",true,Some("http://localhost:8778"),Some(delete)).await.0,StatusCode::BAD_REQUEST);
    assert!(absolute.join("kept.txt").exists());
    let (_,list)=admin.request(&format!("/api/ops/local/files?path={path}"),true).await;
    let revision=list["data"]["items"][0]["revision"].as_str().unwrap();
    let delete=json!({"action":"delete","path":format!("{path}/kept.txt"),"expectedRevision":revision});
    assert_eq!(admin.send("POST","/api/ops/local/files/mutate",true,Some("http://localhost:8778"),Some(delete)).await.0,StatusCode::OK);
    assert!(!absolute.join("kept.txt").exists());
    assert_eq!(admin.request("/api/ops/local/files?path=../outside",true).await.0,StatusCode::BAD_REQUEST);
    let db=Connection::open(&admin.path).unwrap();assert!(db.query_row("SELECT count(*) FROM ops_audit_events WHERE action='files.upload.completed'",[],|r|r.get::<_,i64>(0)).unwrap()>0);
    std::fs::remove_dir_all(absolute).unwrap();
}

#[tokio::test]
async fn generic_container_creation_requires_admin_origin_and_retries_never_contact_docker() {
    let fixture=Fixture::new("platform_admin",false);
    let path="/api/ops/local/containers";
    let id=uuid::Uuid::new_v4().to_string();
    let body=serde_json::json!({"creationId":id,"name":"demo","imageId":format!("sha256:{}","a".repeat(64)),"networkMode":"bridge","networkName":"bridge","restartPolicy":"no","start":false});
    assert_eq!(fixture.send("POST",path,false,Some("http://localhost:8778"),Some(body.clone())).await.0,StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.send("POST",path,true,None,Some(body.clone())).await.0,StatusCode::FORBIDDEN);
    let denied=Fixture::new("platform_operator",false);
    assert_eq!(denied.send("POST",path,true,Some("http://localhost:8778"),Some(body.clone())).await.0,StatusCode::FORBIDDEN);
    let mut invalid=body.clone();invalid["networkMode"]=serde_json::json!("--privileged");
    assert_eq!(fixture.send("POST",path,true,Some("http://localhost:8778"),Some(invalid)).await.0,StatusCode::BAD_REQUEST);
    let db=Connection::open(&fixture.path).unwrap();
    db.execute("INSERT INTO ops_container_creations(id,name,image_id,actor_user_id,status,created_at_ms) VALUES(?,'demo','image','u1','running',1)",[&id]).unwrap();
    let (status,result)=fixture.send("POST",path,true,Some("http://localhost:8778"),Some(body)).await;
    assert_eq!(status,StatusCode::OK);assert_eq!(result["data"]["creationId"],id);
    initialize(&fixture.path).unwrap();
    let (_,result)=fixture.request(&format!("/api/ops/local/container-creations/{id}"),true).await;
    assert_eq!(result["data"]["status"],"interrupted");
    let mut item=serde_json::json!({"id":"a".repeat(64),"creationId":id});
    assert!(!container_create::managed(&fixture.state,&item).await);
    db.execute("UPDATE ops_container_creations SET container_id=? WHERE id=?",params!["a".repeat(64),id]).unwrap();
    assert!(container_create::managed(&fixture.state,&item).await);
    item["id"]=serde_json::json!("b".repeat(64));assert!(!container_create::managed(&fixture.state,&item).await);
}

#[tokio::test]
async fn no_cookie_and_finance_role_cannot_read_nodes_and_all_attempts_are_audited() {
    let fixture = Fixture::new("platform_finance", true);
    assert_eq!(
        fixture.request("/api/ops/nodes", false).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        fixture.request("/api/ops/nodes", true).await.0,
        StatusCode::FORBIDDEN
    );
    let (_, context) = fixture.request("/api/ops/context", true).await;
    assert_eq!(context["data"]["allowed"], false);
    let connection = Connection::open(&fixture.path).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM ops_audit_events", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM ops_audit_events WHERE outcome_status=403",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn disabled_feature_never_contacts_agent_and_offline_is_not_an_empty_success() {
    let disabled = Fixture::new("platform_admin", false);
    let (status, body) = disabled.request("/api/ops/nodes", true).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "OPS_DISABLED");
    let enabled = Fixture::new("platform_admin", true);
    let (status, body) = enabled.request("/api/ops/nodes", true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["items"], serde_json::json!([]));
    assert_eq!(body["data"]["connectionErrorCode"], "OPS_NODE_OFFLINE");
    let (_, context) = enabled.request("/api/ops/context", true).await;
    assert_eq!(context["data"]["writesEnabled"], false);
    assert_eq!(context["data"]["hostTerminalEnabled"], false);
}

#[tokio::test]
async fn local_docker_endpoint_keeps_auth_and_role_checks_when_monitoring_disabled() {
    let fixture = Fixture::new("platform_finance", false);
    assert_eq!(fixture.request("/api/ops/local/containers", false).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.request("/api/ops/local/containers", true).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn container_actions_validate_role_origin_and_target_before_docker_execution() {
    let admin=Fixture::new("platform_admin",false);
    let operator=Fixture::new("platform_finance",false);
    let path=format!("/api/ops/local/containers/{}/actions","a".repeat(64));
    let input=serde_json::json!({"action":"stop","expectedState":"running"});
    assert_eq!(admin.send("POST",&path,false,Some("http://localhost:8778"),Some(input.clone())).await.0,StatusCode::UNAUTHORIZED);
    assert_eq!(operator.send("POST",&path,true,Some("http://localhost:8778"),Some(input.clone())).await.0,StatusCode::FORBIDDEN);
    for origin in [None,Some("https://attacker.example")] {
        assert_eq!(admin.send("POST",&path,true,origin,Some(input.clone())).await.0,StatusCode::FORBIDDEN);
    }
    assert_eq!(admin.send("POST","/api/ops/local/containers/--all/actions",true,Some("http://localhost:8778"),Some(input.clone())).await.0,StatusCode::BAD_REQUEST);
    assert_eq!(admin.send("POST",&path,true,Some("http://localhost:8778"),Some(serde_json::json!({"action":"rm","expectedState":"exited"}))).await.0,StatusCode::BAD_REQUEST);
    let db=Connection::open(&admin.path).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM ops_container_actions",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}

#[test]
fn container_action_lease_and_interruption_survive_restart() {
    let fixture=Fixture::new("platform_admin",false);
    let db=Connection::open(&fixture.path).unwrap();
    db.execute("INSERT INTO ops_container_actions VALUES('t1','container','stop','u1','running',NULL,1,NULL,NULL)",[]).unwrap();
    assert!(db.execute("INSERT INTO ops_container_actions VALUES('t2','container','stop','u1','running',NULL,2,NULL,NULL)",[]).is_err());
    initialize(&fixture.path).unwrap();
    assert_eq!(db.query_row("SELECT status FROM ops_container_actions WHERE id='t1'",[],|r|r.get::<_,String>(0)).unwrap(),"interrupted");
    db.execute("INSERT INTO ops_container_actions VALUES('t2','container','stop','u1','running',NULL,2,NULL,NULL)",[]).unwrap();
}

#[tokio::test]
async fn account_changes_invalidate_existing_cookies_without_server_restart() {
    let fixture = Fixture::new("platform_admin", true);
    assert_eq!(
        fixture.request("/api/ops/context", true).await.0,
        StatusCode::OK
    );
    Connection::open(&fixture.path)
        .unwrap()
        .execute("UPDATE users SET status='inactive'", [])
        .unwrap();
    assert_eq!(
        fixture.request("/api/ops/context", true).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn observability_endpoints_require_admin_and_reject_bad_log_queries_before_docker() {
    let admin=Fixture::new("platform_admin",false);
    let finance=Fixture::new("platform_finance",false);
    let id="a".repeat(64);
    for suffix in ["logs", "metrics", "terminal-sessions", "logs/live", "terminal"] {
        let path=format!("/api/ops/local/containers/{id}/{suffix}");
        assert_eq!(admin.request(&path,false).await.0,StatusCode::UNAUTHORIZED);
        assert_eq!(finance.request(&path,true).await.0,StatusCode::FORBIDDEN);
    }
    let (_,body)=admin.request(&format!("/api/ops/local/containers/{id}/logs?since=20&until=10"),true).await;
    assert!(body["message"].as_str().unwrap().contains("时间范围"));
    assert_eq!(admin.request("/api/ops/local/containers/--all/metrics",true).await.0,StatusCode::BAD_REQUEST);
    let db=Connection::open(&admin.path).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM ops_terminal_sessions",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}

#[test]
fn terminal_connections_are_interrupted_and_audited_once_on_restart() {
    let fixture=Fixture::new("platform_admin",false);
    let db=Connection::open(&fixture.path).unwrap();
    db.execute("INSERT INTO ops_terminal_sessions(id,container_id,actor_user_id,created_at_ms,status,exec_id,shell_pid) VALUES('terminal-test','container','u1',1,'connected','exec','42')",[]).unwrap();
    initialize(&fixture.path).unwrap();initialize(&fixture.path).unwrap();
    let (status,disconnected):(String,Option<i64>)=db.query_row("SELECT status,disconnected_at_ms FROM ops_terminal_sessions WHERE id='terminal-test'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(status,"interrupted");assert!(disconnected.is_some());
    assert_eq!(db.query_row("SELECT count(*) FROM ops_audit_events WHERE action='container.terminal.interrupted'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}
