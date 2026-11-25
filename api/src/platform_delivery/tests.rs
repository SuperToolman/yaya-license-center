use super::*;
use http_body_util::BodyExt;
use tower::ServiceExt;

pub(super) struct Fixture {
    pub(super) state: AppState,
    token: String,
    root: PathBuf,
}
impl Fixture {
    pub(super) fn new(role: &str) -> Self {
        let root = std::env::temp_dir().join(format!("delivery-tests-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("test.sqlite3");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE users(user_id TEXT PRIMARY KEY,username TEXT,password_hash TEXT,role TEXT,organization_id TEXT,status TEXT); CREATE TABLE customers(customer_id TEXT PRIMARY KEY,name TEXT,status TEXT,contact_phone TEXT NOT NULL DEFAULT '13800138000'); INSERT INTO customers(customer_id,name,status) VALUES('c1','客户一','active'),('disabled','禁用客户','inactive');").unwrap();
        let hash = Argon2::default()
            .hash_password(
                b"password",
                &SaltString::encode_b64(b"delivery-test-salt").unwrap(),
            )
            .unwrap()
            .to_string();
        db.execute(
            "INSERT INTO users VALUES('u','admin',?,?,'platform','active')",
            params![hash, role],
        )
        .unwrap();
        db.execute_batch("CREATE TABLE orders(order_id TEXT PRIMARY KEY,customer_id TEXT);CREATE TABLE licenses(license_id TEXT PRIMARY KEY,license TEXT,subject TEXT,customer_name_snapshot TEXT,modules_json TEXT,issued_at INTEGER,expires_at INTEGER,module_expires_at_json TEXT,ai_employees_json TEXT,platform_status TEXT,module_statuses_json TEXT,order_id TEXT,linkage_status TEXT);INSERT INTO orders VALUES('o','c1');INSERT INTO licenses VALUES('lic-test','test','c1','客户一','[\"platform\"]',1,4102444800,'{}','[]','unactivated','{}','o','linked');").unwrap();
        crate::licensing::initialize(&mut ops::db::open(&path).unwrap()).unwrap();
        ops::initialize(&path).unwrap();
        platform_versions::initialize(&path).unwrap();
        initialize(&path).unwrap();
        initialize(&path).unwrap();
        db.execute(
            "INSERT INTO platform_packages VALUES('p1','digest',1,?)",
            [serde_json::json!({"version":"1.13a","filename":"1.13a-p1.zip","artifactType":"offline-image"}).to_string()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO ops_nodes VALUES('n1','agent1',?,1)",
            [serde_json::json!({"hostname":"host1"}).to_string()],
        )
        .unwrap();
        let (token, _) = staff_sessions::login(
            &db,
            LoginRequest {
                username: "admin".into(),
                password: "password".into(),
            },
            Utc::now().timestamp_millis(),
        )
        .unwrap();
        Self {
            state: AppState {
                signing_key: Arc::new(EncodingKey::from_secret(b"test")),
                verification_key: Arc::new(DecodingKey::from_secret(b"test")),
                database_path: path,
                sessions: Arc::new(RwLock::new(HashMap::new())),
                ops: Arc::new(ops::OpsState::new(false, root.join("none"))),
            },
            token,
            root,
        }
    }
    pub(super) async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        authenticated: bool,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .header("origin", "http://localhost:8778");
        if authenticated {
            request = request.header("cookie", format!("license_center_session={}", self.token));
        }
        let response = router()
            .with_state(self.state.clone())
            .oneshot(
                request
                    .body(
                        body.map(|b| Body::from(b.to_string()))
                            .unwrap_or_else(Body::empty),
                    )
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub(super) fn input(name: &str) -> serde_json::Value {
    serde_json::json!({"name":name,"customerId":"c1","packageId":"p1","containerName":"yaya-customer-test","environment":"ALIYUN_ACCESS_KEY_ID=sensitive-test-value","webPort":8787,"apiPort":8788,"publicUrl":"https://customer.example.com","notes":"test"})
}

#[tokio::test]
async fn desktop_installer_checks_session_deployment_and_instance_address() {
    let f = Fixture::new("platform_admin");
    let (status, created) = f
        .call("POST", "/api/platform-instances", Some(input("桌面端测试")), true)
        .await;
    assert!(status.is_success(), "{created}");
    let id = created["data"]["instanceId"].as_str().unwrap();
    let path = format!("/api/platform-instances/{id}/desktop-installer");
    assert_eq!(
        f.call("POST", &path, None, false).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = f.call("POST", &path, None, true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["message"], "请先部署平台实例");

    let db = ops::db::open(&f.state.database_path).unwrap();
    db.execute(
        "UPDATE platform_instances SET container_id=?, public_url='' WHERE instance_id=?",
        params!["a".repeat(64), id],
    )
    .unwrap();
    let (status, body) = f.call("POST", &path, None, true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["message"], "请先为平台实例配置客户可访问的 HTTP(S) 地址");
}

#[test]
fn instance_authorization_rejects_other_customer_expiry_revocation_and_disabled_customer() {
    let f = Fixture::new("platform_admin");
    let db = ops::db::open(&f.state.database_path).unwrap();
    assert!(authorization::eligible(&db, "c1", Some("lic-test")).is_ok());
    assert!(authorization::eligible(&db, "c1", Some("other-customer-license")).is_err());
    assert!(authorization::eligible(&db, "disabled", Some("lic-test")).is_err());
    db.execute("UPDATE customers SET status='inactive' WHERE customer_id='c1'", []).unwrap();
    assert!(authorization::eligible(&db, "c1", None).is_err());
    db.execute("UPDATE customers SET status='active' WHERE customer_id='c1'", []).unwrap();
    db.execute("UPDATE licenses SET expires_at=1", []).unwrap();
    assert!(authorization::eligible(&db, "c1", None).is_err());
    db.execute("UPDATE licenses SET expires_at=4102444800,platform_status='destroyed'", []).unwrap();
    assert!(authorization::eligible(&db, "c1", None).is_err());
}

#[tokio::test]
async fn container_sequence_skips_docker_and_reserved_names_and_allows_repeated_display_names() {
    let f = Fixture::new("platform_admin");
    let base = "lowcode-1.13a-c1";
    let docker_names = HashSet::from([format!("{base}-1"), format!("{base}-3")]);
    let db = ops::db::open(&f.state.database_path).unwrap();
    assert_eq!(next_container_name(&db, base, &HashSet::new()).unwrap(), format!("{base}-1"));
    assert_eq!(next_container_name(&db, base, &docker_names).unwrap(), format!("{base}-2"));
    let mut first = input(base);
    first["containerName"] = serde_json::json!(format!("{base}-2"));
    let (status, first) = f.call("POST", "/api/platform-instances", Some(first), true).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(next_container_name(&db, base, &docker_names).unwrap(), format!("{base}-4"));
    let mut second = input(base);
    second["containerName"] = serde_json::json!(next_container_name(&db, base, &docker_names).unwrap());
    second["webPort"] = serde_json::json!(9000);
    second["apiPort"] = serde_json::json!(9001);
    let (status, second) = f.call("POST", "/api/platform-instances", Some(second), true).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(first["data"]["name"], second["data"]["name"]);
    assert_ne!(first["data"]["instanceId"], second["data"]["instanceId"]);
    assert_eq!(second["data"]["containerName"], format!("{base}-4"));
    db.execute("UPDATE platform_instances SET status='archived'", []).unwrap();
    assert_eq!(next_container_name(&db, base, &docker_names).unwrap(), format!("{base}-5"));
    let long = "x".repeat(80);
    let candidate = next_container_name(&db, &long, &HashSet::new()).unwrap();
    assert_eq!(candidate.len(), 63);
    assert!(candidate.ends_with("-1"));
}

#[tokio::test]
async fn container_sequence_migration_preserves_instances_deployments_and_customer_protection() {
    let f = Fixture::new("platform_admin");
    let (status, first) = f.call("POST", "/api/platform-instances", Some(input("已有实例")), true).await;
    assert_eq!(status, StatusCode::OK);
    let id = first["data"]["instanceId"].as_str().unwrap();
    let db = ops::db::open(&f.state.database_path).unwrap();
    let before: String = db.query_row("SELECT environment_encrypted FROM platform_instances WHERE instance_id=?", [id], |row| row.get(0)).unwrap();
    db.execute("INSERT INTO platform_deployments(deployment_id,instance_id,status,created_by,created_at) VALUES('saved-deployment',?,'failed','u',1)", [id]).unwrap();
    // Reapply the table rebuild with populated parent and child records.
    db.execute_batch("DELETE FROM platform_delivery_migrations WHERE version IN (5,6,7,8); DROP TABLE container_version_tasks; DROP TABLE container_recovery_points; DROP TABLE container_revisions; DROP TABLE platform_license_syncs; ALTER TABLE platform_instances DROP COLUMN license_synced_at; ALTER TABLE platform_instances DROP COLUMN license_sync_error; ALTER TABLE platform_instances DROP COLUMN license_sync_status; ALTER TABLE platform_instances DROP COLUMN applied_license_id; ALTER TABLE platform_instances DROP COLUMN license_id; ALTER TABLE platform_instances DROP COLUMN authorization_id; ALTER TABLE platform_instances DROP COLUMN network_name; ALTER TABLE platform_instances DROP COLUMN network_mode;").unwrap();
    initialize(&f.state.database_path).unwrap();
    initialize(&f.state.database_path).unwrap();
    assert_eq!(get_instance(&db, id).unwrap().container_name, "yaya-customer-test");
    assert_eq!(db.query_row("SELECT environment_encrypted FROM platform_instances WHERE instance_id=?", [id], |row| row.get::<_, String>(0)).unwrap(), before);
    assert_eq!(db.query_row("SELECT instance_id FROM platform_deployments WHERE deployment_id='saved-deployment'", [], |row| row.get::<_, String>(0)).unwrap(), id);
    assert!(!db.prepare("PRAGMA foreign_key_check").unwrap().exists([]).unwrap());
    assert!(db.execute("DELETE FROM customers WHERE customer_id='c1'", []).is_err());
}

#[tokio::test]
async fn package_options_return_each_uploaded_image_environment_snapshot() {
    let f = Fixture::new("platform_admin");
    let db = Connection::open(&f.state.database_path).unwrap();
    db.execute("UPDATE platform_packages SET metadata=? WHERE package_id='p1'", [serde_json::json!({"version":"1.13a","artifactType":"offline-image","imageEnvironment":["PATH=/usr/bin:/bin","PACKAGE_ONLY=value=a=b"]}).to_string()]).unwrap();
    let (status, response) = f.call("GET", "/api/platform-instances/options", None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["data"]["packages"][0]["environment"], serde_json::json!(["PATH=/usr/bin:/bin","PACKAGE_ONLY=value=a=b"]));
}

#[tokio::test]
async fn obsolete_packages_are_not_selectable_and_build_routes_are_removed() {
    let f = Fixture::new("platform_admin");
    let db = Connection::open(&f.state.database_path).unwrap();
    db.execute(
        "UPDATE platform_packages SET metadata=? WHERE package_id='p1'",
        [serde_json::json!({"version":"1.13a","artifactType":"source-bundle"}).to_string()],
    )
    .unwrap();
    let (_, options) = f
        .call("GET", "/api/platform-instances/options", None, true)
        .await;
    assert!(options["data"]["packages"].as_array().unwrap().is_empty());
    assert_eq!(
        f.call(
            "POST",
            "/api/platform-instances",
            Some(input("obsolete")),
            true
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    for path in ["/api/platform-builds", "/api/platform-versions/p1/builds"] {
        let response = router()
            .with_state(f.state.clone())
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn registry_connections_encrypt_password_preserve_credentials_and_switch_default() {
    let f = Fixture::new("platform_admin");
    let path = "/api/platform-registries";
    let (_, data) = f.call("GET", path, None, true).await;
    assert_eq!(data["data"][0]["registryId"], "dockerhub");
    let body = serde_json::json!({"alias":"私有仓库","url":"https://registry.example.com","username":"deployer","password":"registry-secret-value","mirrors":[],"isDefault":true});
    let (status, result) = f.call("POST", path, Some(body.clone()), true).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["data"]["hasPassword"], true);
    assert!(!result.to_string().contains("registry-secret-value"));
    let id = result["data"]["registryId"].as_str().unwrap();
    let db = Connection::open(&f.state.database_path).unwrap();
    let ciphertext: String = db
        .query_row(
            "SELECT credentials FROM platform_registries WHERE registry_id=?",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!ciphertext.contains("registry-secret-value"));
    let mut edit = body.clone();
    edit.as_object_mut().unwrap().remove("password");
    edit["alias"] = serde_json::json!("修改名称");
    assert_eq!(
        f.call("PUT", &format!("{path}/{id}"), Some(edit), true)
            .await
            .0,
        StatusCode::OK
    );
    let ciphertext: String = db
        .query_row(
            "SELECT credentials FROM platform_registries WHERE registry_id=?",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let credentials = environment::decrypt(
        &f.state.database_path,
        &format!("registry:{id}"),
        &ciphertext,
    )
    .unwrap();
    assert_eq!(
        credentials.get("password").map(String::as_str),
        Some("registry-secret-value")
    );
    assert_eq!(
        f.call("DELETE", &format!("{path}/{id}"), None, true)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let hub = serde_json::json!({"alias":"DockerHub","url":"https://hub.docker.com","username":"","mirrors":["https://mirror.example.com"],"isDefault":true});
    assert_eq!(
        f.call("PUT", &format!("{path}/dockerhub"), Some(hub), true)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call("DELETE", &format!("{path}/{id}"), None, true)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call("DELETE", &format!("{path}/dockerhub"), None, true)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn registry_settings_reject_unauthorized_and_invalid_updates() {
    for role in ["platform_operator", "platform_admin"] {
        let f = Fixture::new(role);
        assert_eq!(
            f.call("GET", "/api/platform-registries", None, false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        if role == "platform_operator" {
            assert_eq!(
                f.call("GET", "/api/platform-registries", None, true)
                    .await
                    .0,
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                f.call(
                    "POST",
                    "/api/platform-registries",
                    Some(serde_json::json!({"alias":"test","url":"https://registry.example.com"})),
                    true
                )
                .await
                .0,
                StatusCode::FORBIDDEN
            );
        } else {
            for mirrors in [
                serde_json::json!(["http://mirror.example.com"]),
                serde_json::json!(["https://mirror.example.com", "https://mirror.example.com/"]),
            ] {
                assert_eq!(f.call("PUT","/api/platform-registries/dockerhub",Some(serde_json::json!({"alias":"DockerHub","url":"https://hub.docker.com","mirrors":mirrors})),true).await.0,StatusCode::BAD_REQUEST);
            }
        }
    }
}

#[tokio::test]
async fn local_instances_validate_configuration_ports_and_archive_with_audit() {
    let f = Fixture::new("platform_admin");
    let (status, result) = f
        .call(
            "POST",
            "/api/platform-instances",
            Some(input("平台一")),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["data"]["status"], "not_deployed");
    assert_eq!(result["data"]["containerName"], "yaya-customer-test");
    assert!(!result.to_string().contains("sensitive-test-value"));
    let id = result["data"]["instanceId"].as_str().unwrap();
    assert_eq!(
        f.call(
            "POST",
            "/api/platform-instances",
            Some(input("平台二")),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    for (field, value) in [
        ("customerId", serde_json::json!("disabled")),
        ("packageId", serde_json::json!("missing")),
        ("containerName", serde_json::json!("invalid;name")),
        ("environment", serde_json::json!("APP_PORT=1")),
        ("webPort", serde_json::json!(8788)),
        ("publicUrl", serde_json::json!("javascript:alert(1)")),
    ] {
        let mut body = input("其他平台");
        body[field] = value;
        assert_eq!(
            f.call("POST", "/api/platform-instances", Some(body), true)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    let db = Connection::open(&f.state.database_path).unwrap();
    assert!(
        db.execute("DELETE FROM customers WHERE customer_id='c1'", [])
            .is_err()
    );
    let (_, detail) = f
        .call("GET", &format!("/api/platform-instances/{id}"), None, true)
        .await;
    assert_eq!(detail["data"]["status"], "not_deployed");
    let mut edited = input("改名平台");
    edited["webPort"] = serde_json::json!(9000);
    edited["containerName"] = serde_json::json!("yaya-renamed");
    edited["publicUrl"] = serde_json::json!("");
    assert_eq!(
        f.call(
            "PUT",
            &format!("/api/platform-instances/{id}"),
            Some(edited.clone()),
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call(
            "POST",
            &format!("/api/platform-instances/{id}/archive"),
            None,
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call(
            "PUT",
            &format!("/api/platform-instances/{id}"),
            Some(edited),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        f.call(
            "POST",
            "/api/platform-instances",
            Some(input("平台二")),
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, list) = f.call("GET", "/api/platform-instances", None, true).await;
    assert_eq!(list["data"].as_array().unwrap().len(), 2);
    let (_, options) = f
        .call("GET", "/api/platform-instances/options", None, true)
        .await;
    assert_eq!(options["data"]["customers"].as_array().unwrap().len(), 1);
    assert_eq!(
        db.query_row("SELECT count(*) FROM platform_delivery_events", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        4
    );
}

#[tokio::test]
async fn rejects_unauthorized_delivery_endpoints() {
    for role in ["platform_operator", "platform_admin"] {
        let f = Fixture::new(role);
        for (method, path, body) in [
            ("GET", "/api/platform-runtime", None),
            ("POST", "/api/platform-instances/missing/deploy", None),
            ("GET", "/api/platform-instances/missing/deployments", None),
            ("GET", "/api/platform-instances/options", None),
            ("GET", "/api/platform-instances", None),
            ("POST", "/api/platform-instances", Some(input("x"))),
            ("GET", "/api/platform-instances/missing", None),
            ("PUT", "/api/platform-instances/missing", Some(input("x"))),
            ("POST", "/api/platform-instances/missing/archive", None),
        ] {
            assert_eq!(
                f.call(method, path, body, role != "platform_admin").await.0,
                if role == "platform_admin" {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::FORBIDDEN
                }
            );
        }
    }
}

#[tokio::test]
async fn runtime_is_honest_and_docker_absence_does_not_queue_deployments() {
    let f = Fixture::new("platform_admin");
    let (status, runtime) = f.call("GET", "/api/platform-runtime", None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(runtime["data"]["available"].is_boolean());
    if runtime["data"]["available"] == false {
        assert_eq!(
            f.call("POST", "/api/platform-instances/missing/deploy", None, true)
                .await
                .0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        let db = Connection::open(&f.state.database_path).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM platform_deployments", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn interrupted_deployment_is_preserved_as_failed_with_container_identity() {
    let f = Fixture::new("platform_admin");
    let (_, value) = f
        .call(
            "POST",
            "/api/platform-instances",
            Some(input("客户平台")),
            true,
        )
        .await;
    let id = value["data"]["instanceId"].as_str().unwrap();
    let db = Connection::open(&f.state.database_path).unwrap();
    db.execute("INSERT INTO platform_deployments(deployment_id,instance_id,status,created_by,created_at) VALUES('interrupted',?,'running','u',1)",[id]).unwrap();
    db.execute("UPDATE platform_instances SET deployment_status='deploying',container_id='container-before-restart' WHERE instance_id=?",[id]).unwrap();
    recover(&f.state.database_path).unwrap();
    let (_, value) = f
        .call("GET", &format!("/api/platform-instances/{id}"), None, true)
        .await;
    assert_eq!(value["data"]["status"], "failed");
    assert_eq!(value["data"]["containerId"], "container-before-restart");
    let (_, tasks) = f
        .call(
            "GET",
            &format!("/api/platform-instances/{id}/deployments"),
            None,
            true,
        )
        .await;
    assert_eq!(tasks["data"][0]["status"], "interrupted");
    assert_eq!(
        f.call(
            "PUT",
            &format!("/api/platform-instances/{id}"),
            Some(input("不能修改")),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let cipher: String = db
        .query_row(
            "SELECT environment_encrypted FROM platform_instances WHERE instance_id=?",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!cipher.contains("sensitive-test-value"));
}

#[tokio::test]
async fn instance_network_defaults_persist_and_invalid_modes_do_not_create_records(){
    let f=Fixture::new("platform_admin");
    let (status,body)=f.call("POST","/api/platform-instances",Some(input("网络实例")),true).await;
    assert_eq!(status,StatusCode::OK,"{body}");
    assert_eq!(body["data"]["networkMode"],"bridge");assert_eq!(body["data"]["networkName"],"bridge");
    let id=body["data"]["instanceId"].as_str().unwrap();
    let (status,body)=f.call("GET",&format!("/api/platform-instances/{id}"),None,true).await;
    assert_eq!(status,StatusCode::OK);assert_eq!(body["data"]["networkName"],"bridge");
    for (mode,name) in [("host","host"),("none","none"),("bridge","--help"),("macvlan","")] {
        let mut v=input("非法网络");v["networkMode"]=serde_json::json!(mode);v["networkName"]=serde_json::json!(name);
        assert_eq!(f.call("POST","/api/platform-instances",Some(v),true).await.0,StatusCode::BAD_REQUEST);
    }
    assert_eq!(f.call("GET","/api/platform-networks",None,false).await.0,StatusCode::UNAUTHORIZED);
    let other=Fixture::new("platform_operator");assert_eq!(other.call("GET","/api/platform-networks",None,true).await.0,StatusCode::FORBIDDEN);
}
