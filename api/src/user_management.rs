//! Fixed-role staff account maintenance; business history keeps the original actor IDs.
use super::*;
use rusqlite::{Transaction, TransactionBehavior};

type ApiError = (StatusCode, Json<Envelope<()>>);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpdateUser {
    display_name: String,
    role: String,
    organization_id: String,
    password: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct UserStatus {
    status: String,
}

fn db_error(_: rusqlite::Error) -> ApiError {
    error(StatusCode::INTERNAL_SERVER_ERROR, "账号操作失败")
}

fn read(db: &Connection, id: &str) -> Result<UserView, ApiError> {
    db.query_row("SELECT user_id, username, display_name, role, organization_id, status FROM users WHERE user_id=?", [id], |r| Ok(UserView {
        user_id:r.get(0)?, username:r.get(1)?, display_name:r.get(2)?, role:r.get(3)?, organization_id:r.get(4)?, status:r.get(5)?,
    })).optional().map_err(db_error)?.ok_or_else(|| error(StatusCode::NOT_FOUND, "账号不存在"))
}

// Recheck the actor under the same write lock as the change. This serializes
// concurrent demotions/deletions so they cannot remove the last active admin.
fn authorize(db: &Connection, actor: &str) -> Result<(), ApiError> {
    let user = read(db, actor)?;
    if user.role != "platform_admin" || user.status != "active" {
        return Err(error(StatusCode::FORBIDDEN, "仅平台超级管理员可管理账号"));
    }
    Ok(())
}

fn protect(
    db: &Connection,
    actor: &str,
    user: &UserView,
    loses_admin: bool,
) -> Result<(), ApiError> {
    if actor == user.user_id {
        return Err(error(
            StatusCode::CONFLICT,
            "不能停用、删除或变更当前账号的角色、组织及密码",
        ));
    }
    if loses_admin && user.role == "platform_admin" && user.status == "active" {
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM users WHERE role='platform_admin' AND status='active'",
                [],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if count <= 1 {
            return Err(error(
                StatusCode::CONFLICT,
                "必须保留至少一个启用的超级管理员",
            ));
        }
    }
    Ok(())
}

fn update_account(
    tx: &Transaction<'_>,
    actor: &str,
    id: &str,
    request: UpdateUser,
) -> Result<UserView, ApiError> {
    authorize(tx, actor)?;
    let user = read(tx, id)?;
    if !FIXED_ROLES.contains(&request.role.as_str()) || request.organization_id.trim().is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "请选择固定角色并填写所属组织",
        ));
    }
    let password = request.password.filter(|v| !v.is_empty());
    if user.role != request.role
        || user.organization_id != request.organization_id.trim()
        || password.is_some()
    {
        protect(tx, actor, &user, request.role != "platform_admin")?;
    }
    let password_hash = password
        .map(|value| {
            let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "密码处理失败"))?;
            Argon2::default()
                .hash_password(value.as_bytes(), &salt)
                .map(|v| v.to_string())
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "密码处理失败"))
        })
        .transpose()?;
    tx.execute("UPDATE users SET display_name=?,role=?,organization_id=?,password_hash=COALESCE(?,password_hash) WHERE user_id=?", params![request.display_name.trim(),request.role,request.organization_id.trim(),password_hash,id]).map_err(db_error)?;
    read(tx, id)
}

fn change_status(
    tx: &Transaction<'_>,
    actor: &str,
    id: &str,
    status: &str,
) -> Result<UserView, ApiError> {
    authorize(tx, actor)?;
    let user = read(tx, id)?;
    if !["active", "inactive"].contains(&status) {
        return Err(error(StatusCode::BAD_REQUEST, "账号状态无效"));
    }
    protect(tx, actor, &user, status == "inactive")?;
    tx.execute(
        "UPDATE users SET status=? WHERE user_id=?",
        params![status, id],
    )
    .map_err(db_error)?;
    read(tx, id)
}

fn delete_account(tx: &Transaction<'_>, actor: &str, id: &str) -> Result<(), ApiError> {
    authorize(tx, actor)?;
    let user = read(tx, id)?;
    protect(tx, actor, &user, true)?;
    if user.status != "inactive" {
        return Err(error(StatusCode::CONFLICT, "请先停用账号再删除"));
    }
    tx.execute("DELETE FROM staff_sessions WHERE user_id=?", [id])
        .map_err(db_error)?;
    tx.execute("DELETE FROM users WHERE user_id=?", [id])
        .map_err(db_error)?;
    Ok(())
}

async fn mutate<T: Send + 'static>(
    state: AppState,
    action: impl FnOnce(&Transaction<'_>) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(move || {
        let mut db = Connection::open(state.database_path).map_err(db_error)?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db_error)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let result = action(&tx)?;
        tx.commit().map_err(db_error)?;
        Ok(result)
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "账号操作失败"))?
}

pub(super) async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<UpdateUser>,
) -> Result<Json<Envelope<UserView>>, ApiError> {
    let actor = require_platform_admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let user = mutate(state, move |tx| {
        update_account(tx, &actor.user_id, &id, request)
    })
    .await?;
    Ok(Json(success("账号已更新", user)))
}

pub(super) async fn set_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<UserStatus>,
) -> Result<Json<Envelope<UserView>>, ApiError> {
    let actor = require_platform_admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let user = mutate(state, move |tx| {
        change_status(tx, &actor.user_id, &id, &request.status)
    })
    .await?;
    Ok(Json(success("账号状态已更新", user)))
}

pub(super) async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<()>>, ApiError> {
    let actor = require_platform_admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    mutate(state, move |tx| delete_account(tx, &actor.user_id, &id)).await?;
    Ok(Json(success("账号已删除", ())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    struct Fixture {
        state: AppState,
        root: PathBuf,
        token: String,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("user-management-{}", Uuid::new_v4()));
            let path = root.join("test.sqlite");
            initialize_database(
                &path,
                &root.join("licenses.json"),
                &root.join("revoked.json"),
                "admin",
                Some("test-password"),
            )
            .unwrap();
            ops::initialize(&path).unwrap();
            let db = Connection::open(&path).unwrap();
            let (token, _) = staff_sessions::login(
                &db,
                LoginRequest {
                    username: "admin".into(),
                    password: "test-password".into(),
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
                    ops: Arc::new(ops::OpsState::new(false, root.join("no-agent"))),
                },
                root,
                token,
            }
        }
        async fn request(
            &self,
            method: &str,
            path: &str,
            body: serde_json::Value,
            token: Option<&str>,
            origin: &str,
        ) -> (StatusCode, serde_json::Value) {
            let router = Router::new()
                .route("/api/users", get(list_users).post(create_user))
                .route(
                    "/api/users/{user_id}",
                    axum::routing::put(update).delete(remove),
                )
                .route("/api/users/{user_id}/status", post(set_status))
                .with_state(self.state.clone());
            let mut req = Request::builder()
                .method(method)
                .uri(path)
                .header("origin", origin)
                .header("content-type", "application/json");
            if let Some(token) = token {
                req = req.header("cookie", format!("license_center_session={token}"));
            }
            let response = router
                .oneshot(req.body(Body::from(body.to_string())).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice(&bytes).unwrap())
        }
        async fn admin(
            &self,
            method: &str,
            path: &str,
            body: serde_json::Value,
        ) -> (StatusCode, serde_json::Value) {
            self.request(
                method,
                path,
                body,
                Some(&self.token),
                "http://localhost:8778",
            )
            .await
        }
        async fn create(&self, username: &str, role: &str) -> String {
            let (status, body) = self.admin("POST", "/api/users", serde_json::json!({"username":username,"password":"test-password","displayName":"测试账号","role":role,"organizationId":"platform"})).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            body["data"]["userId"].as_str().unwrap().into()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn edit(role: &str) -> serde_json::Value {
        serde_json::json!({"displayName":"更新姓名","role":role,"organizationId":"platform"})
    }

    #[tokio::test]
    async fn account_lifecycle_revokes_sessions_and_preserves_history() {
        let f = Fixture::new();
        let id = f.create("operator", "platform_operator").await;
        let path = format!("/api/users/{id}");
        let db = Connection::open(&f.state.database_path).unwrap();
        let now = Utc::now().timestamp_millis();
        let login = || {
            staff_sessions::login(
                &db,
                LoginRequest {
                    username: "operator".into(),
                    password: "test-password".into(),
                },
                now,
            )
            .unwrap()
            .0
        };
        let token = login();
        assert_eq!(
            f.admin("PUT", &path, edit("platform_operator")).await.0,
            StatusCode::OK
        );
        assert!(
            staff_sessions::authenticate(&db, &token, now)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            f.admin("PUT", &path, edit("platform_finance")).await.0,
            StatusCode::OK
        );
        assert!(
            staff_sessions::authenticate(&db, &token, now)
                .unwrap()
                .is_none()
        );
        let token = login();
        assert_eq!(
            f.admin("DELETE", &path, serde_json::Value::Null).await.0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            f.admin(
                "POST",
                &format!("{path}/status"),
                serde_json::json!({"status":"inactive"})
            )
            .await
            .0,
            StatusCode::OK
        );
        assert!(
            staff_sessions::authenticate(&db, &token, now)
                .unwrap()
                .is_none()
        );
        assert!(
            staff_sessions::login(
                &db,
                LoginRequest {
                    username: "operator".into(),
                    password: "test-password".into()
                },
                now
            )
            .is_err()
        );
        assert_eq!(
            f.admin(
                "POST",
                &format!("{path}/status"),
                serde_json::json!({"status":"active"})
            )
            .await
            .0,
            StatusCode::OK
        );
        assert!(
            staff_sessions::authenticate(&db, &token, now)
                .unwrap()
                .is_none()
        );
        let token = login();
        let mut body = edit("platform_finance");
        body["password"] = "new-password".into();
        assert_eq!(f.admin("PUT", &path, body).await.0, StatusCode::OK);
        assert!(
            staff_sessions::authenticate(&db, &token, now)
                .unwrap()
                .is_none()
        );
        assert!(
            staff_sessions::login(
                &db,
                LoginRequest {
                    username: "operator".into(),
                    password: "new-password".into()
                },
                now
            )
            .is_ok()
        );
        // A historical business actor reference must survive account removal.
        db.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) VALUES('history',?,'read','/api/ops/nodes',200,'request',1)", [&id])
            .unwrap();
        f.admin(
            "POST",
            &format!("{path}/status"),
            serde_json::json!({"status":"inactive"}),
        )
        .await;
        assert_eq!(
            f.admin("DELETE", &path, serde_json::Value::Null).await.0,
            StatusCode::OK
        );
        assert!(read(&db, &id).is_err());
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM staff_sessions WHERE user_id=?",
                [&id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT actor_user_id FROM ops_audit_events WHERE id='history'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            id
        );
        assert_eq!(
            f.admin("DELETE", &path, serde_json::Value::Null).await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn authorization_validation_and_self_protection() {
        let f = Fixture::new();
        let id = f.create("operator", "platform_operator").await;
        let path = format!("/api/users/{id}");
        let db = Connection::open(&f.state.database_path).unwrap();
        let (token, _) = staff_sessions::login(
            &db,
            LoginRequest {
                username: "operator".into(),
                password: "test-password".into(),
            },
            Utc::now().timestamp_millis(),
        )
        .unwrap();
        for (method, endpoint, body) in [
            ("PUT", path.clone(), edit("platform_finance")),
            (
                "POST",
                format!("{path}/status"),
                serde_json::json!({"status":"inactive"}),
            ),
            ("DELETE", path.clone(), serde_json::Value::Null),
            (
                "POST",
                "/api/users".into(),
                serde_json::json!({"username":"new","password":"pw","displayName":"","role":"platform_admin"}),
            ),
        ] {
            assert_eq!(
                f.request(
                    method,
                    &endpoint,
                    body.clone(),
                    None,
                    "http://localhost:8778"
                )
                .await
                .0,
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                f.request(
                    method,
                    &endpoint,
                    body.clone(),
                    Some(&token),
                    "http://localhost:8778"
                )
                .await
                .0,
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                f.request(
                    method,
                    &endpoint,
                    body,
                    Some(&f.token),
                    "https://evil.example"
                )
                .await
                .0,
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            f.admin("PUT", &path, edit("unknown")).await.0,
            StatusCode::BAD_REQUEST
        );
        let mut body = edit("platform_operator");
        body["organizationId"] = " ".into();
        assert_eq!(f.admin("PUT", &path, body).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            f.admin(
                "POST",
                &format!("{path}/status"),
                serde_json::json!({"status":"deleted"})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let own: String = db
            .query_row(
                "SELECT user_id FROM users WHERE username='admin'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let own_path = format!("/api/users/{own}");
        assert_eq!(
            f.admin("PUT", &own_path, edit("platform_admin")).await.0,
            StatusCode::OK
        );
        assert_eq!(
            f.admin("PUT", &own_path, edit("platform_operator")).await.0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            f.admin(
                "POST",
                &format!("{own_path}/status"),
                serde_json::json!({"status":"inactive"})
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            f.admin("DELETE", &own_path, serde_json::Value::Null)
                .await
                .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            protect(&db, "other", &read(&db, &own).unwrap(), true)
                .err()
                .unwrap()
                .0,
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn concurrent_admin_demotions_keep_one_active_admin() {
        let f = Fixture::new();
        let second_id = f.create("second-admin", "platform_admin").await;
        let db = Connection::open(&f.state.database_path).unwrap();
        let first_id: String = db
            .query_row(
                "SELECT user_id FROM users WHERE username='admin'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let (token, _) = staff_sessions::login(
            &db,
            LoginRequest {
                username: "second-admin".into(),
                password: "test-password".into(),
            },
            Utc::now().timestamp_millis(),
        )
        .unwrap();
        let first_path = format!("/api/users/{first_id}");
        let second_path = format!("/api/users/{second_id}");
        let (a, b) = tokio::join!(
            f.admin("PUT", &second_path, edit("platform_operator")),
            f.request(
                "PUT",
                &first_path,
                edit("platform_operator"),
                Some(&token),
                "http://localhost:8778"
            )
        );
        assert_eq!(
            [a.0, b.0].iter().filter(|s| **s == StatusCode::OK).count(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM users WHERE role='platform_admin' AND status='active'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}
