use super::*;

#[tokio::test]
async fn employee_knowledge_sqlite_lifecycle() {
    let root = std::env::temp_dir().join(format!("knowledge-test-{}", Uuid::new_v4()));
    let database_path = root.join("catalog.sqlite");
    let missing = root.join("missing.json");
    initialize_database(
        &database_path,
        &missing,
        &missing,
        "admin",
        Some("test-only-password"),
    )
    .unwrap();
    initialize_database(
        &database_path,
        &missing,
        &missing,
        "admin",
        Some("test-only-password"),
    )
    .unwrap();
    let c = Connection::open(&database_path).unwrap();
    c.execute("INSERT INTO ai_employee_products(id,title,price_cents,created_at,updated_at) VALUES('knowledge-test','Knowledge test',0,1,1)", []).unwrap();
    crate::ops::initialize(&database_path).unwrap();
    let (token,_) = crate::staff_sessions::login(&c,crate::LoginRequest { username:"admin".into(),password:"test-only-password".into() },Utc::now().timestamp_millis()).unwrap();
    let state = AppState {
        signing_key: Arc::new(EncodingKey::from_secret(b"test-only")),
        verification_key: Arc::new(DecodingKey::from_secret(b"test-only")),
        database_path,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        ops: Arc::new(crate::ops::OpsState::new(false,PathBuf::from("unused-agent.sock"))),
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        "cookie",
        HeaderValue::from_str(&format!("license_center_session={token}")).unwrap(),
    );
    let employee = "knowledge-test".to_string();
    let request_id = Uuid::new_v4().to_string();
    let put = |name: &str, key: &str, bytes: &'static [u8], h: HeaderMap| {
        upload(
            State(state.clone()),
            h,
            Path(employee.clone()),
            Query(Upload {
                name: name.into(),
                client_request_id: key.into(),
            }),
            Bytes::from_static(bytes),
        )
    };
    assert_eq!(
        put("test.txt", &request_id, b"test", HeaderMap::new())
            .await
            .err()
            .unwrap()
            .0,
        StatusCode::UNAUTHORIZED
    );
    let first = put("test.txt", &request_id, b"test", headers.clone())
        .await
        .unwrap_or_else(|e| panic!("upload {}", e.0))
        .0
        .data
        .unwrap();
    assert_eq!(first.version, "1.0.1");
    assert_eq!(first.documents.len(), 1);
    assert_eq!(
        first.documents[0].sha256,
        format!("{:x}", Sha256::digest(b"test"))
    );
    let repeated = put("test.txt", &request_id, b"test", headers.clone())
        .await
        .unwrap_or_else(|e| panic!("repeat {}", e.0))
        .0
        .data
        .unwrap();
    assert_eq!(repeated.version, first.version);
    assert_eq!(repeated.documents[0].id, first.documents[0].id);
    assert_eq!(
        put("test.txt", &request_id, b"different", headers.clone())
            .await
            .err()
            .unwrap()
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        put(
            "fake.pdf",
            &Uuid::new_v4().to_string(),
            b"not pdf",
            headers.clone()
        )
        .await
        .err()
        .unwrap()
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        put(
            "../secret.txt",
            &Uuid::new_v4().to_string(),
            b"test",
            headers.clone()
        )
        .await
        .err()
        .unwrap()
        .0,
        StatusCode::BAD_REQUEST
    );
    let listed = list(
        State(state.clone()),
        headers.clone(),
        Path(employee.clone()),
    )
    .await
    .unwrap_or_else(|e| panic!("list {}", e.0))
    .0
    .data
    .unwrap();
    assert_eq!(listed.documents.len(), 1);
    assert!(!serde_json::to_string(&listed).unwrap().contains("contents"));
    assert_eq!(
        download(
            State(state.clone()),
            HeaderMap::new(),
            Path((employee.clone(), first.documents[0].id.clone()))
        )
        .await
        .err()
        .unwrap()
        .0,
        StatusCode::UNAUTHORIZED
    );
    let removed = remove(
        State(state.clone()),
        headers.clone(),
        Path((employee.clone(), first.documents[0].id.clone())),
    )
    .await
    .unwrap_or_else(|e| panic!("remove {}", e.0))
    .0
    .data
    .unwrap();
    assert_eq!(removed.version, "1.0.2");
    assert!(removed.documents.is_empty());
    let size: i64 = c
        .query_row(
            "SELECT length(contents) FROM employee_knowledge_documents",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(size, 0);
    let repeated = remove(
        State(state.clone()),
        headers,
        Path((employee.clone(), first.documents[0].id.clone())),
    )
    .await
    .unwrap_or_else(|e| panic!("remove again {}", e.0))
    .0
    .data
    .unwrap();
    assert_eq!(repeated.version, removed.version);
    c.execute("DELETE FROM ai_employee_products WHERE id=?", [&employee])
        .unwrap();
    let count: i64 = c
        .query_row(
            "SELECT count(*) FROM employee_knowledge_documents",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    drop(c);
    drop(state);
    std::fs::remove_dir_all(&root).unwrap();
}
