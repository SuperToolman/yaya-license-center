use super::*;
use http_body_util::BodyExt;
use tower::ServiceExt;

const IMAGE_BYTES: &[u8] = b"fixture-docker-save-archive";
fn image_manifest() -> serde_json::Value {
    let package_id = Uuid::new_v4().to_string();
    serde_json::json!({
        "packageType":"yaya-platform", "formatVersion":3, "artifactType":"offline-image",
        "version":"1.13a", "packageId":package_id, "builtAt":"2026-11-09T00:00:00Z",
        "image":{"id":format!("sha256:{}","a".repeat(64)), "tag":format!("yaya-platform:1.13a-{package_id}"),
        "platform":"linux/amd64","archive":"image.tar","size":IMAGE_BYTES.len(),"sha256":format!("{:x}",Sha256::digest(IMAGE_BYTES))}
    })
}
fn archive_for(manifest: &serde_json::Value, bytes: Option<&[u8]>, extra: Option<&str>) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("manifest.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(manifest.to_string().as_bytes()).unwrap();
    if let Some(bytes) = bytes {
        zip.start_file("image.tar", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    if let Some(name) = extra {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(b"x").unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn make_zip(tamper: bool, extra: Option<&str>) -> Vec<u8> {
    archive_for(
        &image_manifest(),
        Some(if tamper {
            b"tampered-image"
        } else {
            IMAGE_BYTES
        }),
        extra,
    )
}
#[test]
fn rejects_source_registry_formats_and_obsolete_manifest_fields() {
    for change in [
        serde_json::json!({"formatVersion":1,"artifactType":"source-bundle"}),
        serde_json::json!({"formatVersion":2,"artifactType":"container-image"}),
        serde_json::json!({"build":{"dockerfile":"deploy/Dockerfile.single"}}),
        serde_json::json!({"files":[]}),
    ] {
        let mut manifest = image_manifest();
        manifest
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        let temp = Temporary(
            std::env::temp_dir().join(format!("rejected-platform-{}.zip", Uuid::new_v4())),
        );
        fs::write(&temp.0, archive_for(&manifest, Some(IMAGE_BYTES), None)).unwrap();
        assert!(validate(&temp.0).is_err());
    }
}
#[test]
fn validates_payload_hash_and_rejects_missing_images_and_unsafe_paths() {
    for (tamper, extra, valid) in [
        (false, None, true),
        (true, None, false),
        (false, Some("../escape"), false),
        (false, Some("unlisted.txt"), false),
        (false, Some("C:/evil"), false),
    ] {
        let temp =
            Temporary(std::env::temp_dir().join(format!("platform-test-{}.zip", Uuid::new_v4())));
        fs::write(&temp.0, make_zip(tamper, extra)).unwrap();
        assert_eq!(validate(&temp.0).is_ok(), valid);
    }
    let temp =
        Temporary(std::env::temp_dir().join(format!("missing-image-{}.zip", Uuid::new_v4())));
    fs::write(&temp.0, archive_for(&image_manifest(), None, None)).unwrap();
    assert!(validate(&temp.0).is_err());
}
#[test]
fn validates_image_identity_platform_and_expansion_limits() {
    for (field, value) in [
        ("id", serde_json::json!("latest")),
        ("platform", serde_json::json!("windows/amd64")),
        ("archive", serde_json::json!("../image.tar")),
        ("size", serde_json::json!(MAX_IMAGE + 1)),
        ("size", serde_json::json!(0)),
        ("sha256", serde_json::json!("b".repeat(64))),
        ("tag", serde_json::json!("yaya-platform:latest")),
    ] {
        let mut manifest = image_manifest();
        manifest["image"][field] = value;
        let temp =
            Temporary(std::env::temp_dir().join(format!("invalid-image-{}.zip", Uuid::new_v4())));
        fs::write(&temp.0, archive_for(&manifest, Some(IMAGE_BYTES), None)).unwrap();
        assert!(validate(&temp.0).is_err());
    }
}
#[test]
fn unpacks_only_verified_payload_and_refuses_overwrite() {
    let temp =
        Temporary(std::env::temp_dir().join(format!("unpack-platform-{}.zip", Uuid::new_v4())));
    let target =
        Temporary(std::env::temp_dir().join(format!("unpacked-image-{}.tar", Uuid::new_v4())));
    fs::write(&temp.0, make_zip(false, None)).unwrap();
    unpack_image(&temp.0, &target.0).unwrap();
    assert_eq!(fs::read(&target.0).unwrap(), IMAGE_BYTES);
    assert!(unpack_image(&temp.0, &target.0).is_err());
    fs::remove_file(&target.0).unwrap();
    fs::write(&temp.0, make_zip(true, None)).unwrap();
    assert!(unpack_image(&temp.0, &target.0).is_err());
    assert!(!target.0.exists());
}

#[test]
fn image_environment_snapshot_is_optional_bounded_and_validated() {
    for (environment, valid) in [
        (serde_json::json!(["PATH=/usr/bin:/bin", "VERSION_ONLY=value=a=b"]), true),
        (serde_json::json!(["DUPLICATE=a", "DUPLICATE=b"]), false),
        (serde_json::json!(["BROKEN"]), false),
        (serde_json::json!(["KEY=line\nbreak"]), false),
    ] {
        let mut manifest = image_manifest();
        manifest["image"]["environment"] = environment;
        let temp = Temporary(std::env::temp_dir().join(format!("environment-{}.zip", Uuid::new_v4())));
        fs::write(&temp.0, archive_for(&manifest, Some(IMAGE_BYTES), None)).unwrap();
        assert_eq!(validate(&temp.0).is_ok(), valid);
    }
}
#[test]
fn actual_package_from_script_passes_contract_if_provided() {
    if let Some(path) = std::env::var_os("YAYA_TEST_PLATFORM_PACKAGE") {
        let manifest =
            validate(FilePath::new(&path)).expect("real Node-produced offline image ZIP contract");
        assert_eq!(manifest.artifact_type, "offline-image");
        assert!(manifest.image.size > 0);
    }
}

struct Fixture {
    state: AppState,
    token: String,
    root: PathBuf,
}
impl Fixture {
    fn new(role: &str) -> Self {
        let root = std::env::temp_dir().join(format!("platform-api-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("test.sqlite3");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE users(user_id TEXT PRIMARY KEY,username TEXT,password_hash TEXT,role TEXT,organization_id TEXT,status TEXT,display_name TEXT NOT NULL DEFAULT '版本管理员');").unwrap();
        let password = Argon2::default()
            .hash_password(
                b"password",
                &SaltString::encode_b64(b"platform-test-salt").unwrap(),
            )
            .unwrap()
            .to_string();
        db.execute(
            "INSERT INTO users(user_id,username,password_hash,role,organization_id,status) VALUES('u','admin',?,?,'platform','active')",
            params![password, role],
        )
        .unwrap();
        ops::initialize(&path).unwrap();
        initialize(&path).unwrap();
        initialize(&path).unwrap();
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
                ops: Arc::new(ops::OpsState::new(false, root.join("no-agent"))),
            },
            token,
            root,
        }
    }
    async fn request(
        &self,
        method: &str,
        path: &str,
        package: Option<&[u8]>,
        authenticated: bool,
        origin: &str,
    ) -> (StatusCode, Bytes) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("origin", origin);
        if authenticated {
            req = req.header("cookie", format!("license_center_session={}", self.token));
        }
        let body = if let Some(bytes) = package {
            req = req.header("content-type", "multipart/form-data; boundary=yaya-test");
            let mut body=b"--yaya-test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"ignored.zip\"\r\nContent-Type: application/zip\r\n\r\n".to_vec();
            body.extend_from_slice(bytes);
            body.extend_from_slice(b"\r\n--yaya-test--\r\n");
            Body::from(body)
        } else {
            Body::empty()
        };
        let response = router()
            .layer(DefaultBodyLimit::max(11 * 1024 * 1024))
            .with_state(self.state.clone())
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        (
            response.status(),
            response.into_body().collect().await.unwrap().to_bytes(),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn upload_persists_lists_downloads_deduplicates_and_cleans_failed_uploads() {
    let fixture = Fixture::new("platform_admin");
    let mut manifest = image_manifest();
    manifest["image"]["environment"] = serde_json::json!(["PACKAGE_ONLY=value=a=b"]);
    let generated = archive_for(&manifest, Some(IMAGE_BYTES), None);
    let real = std::env::var_os("YAYA_TEST_PLATFORM_PACKAGE").map(|p| fs::read(p).unwrap());
    let bytes = real.as_deref().unwrap_or(&generated);
    let request = || {
        fixture.request(
            "POST",
            "/api/platform-versions",
            Some(bytes),
            true,
            "http://localhost:8778",
        )
    };
    let (status, payload) = request().await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&payload)
    );
    let record: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(record["data"]["uploadedByName"], "版本管理员");
    if real.is_none() {
        assert_eq!(record["data"]["imageEnvironment"], serde_json::json!(["PACKAGE_ONLY=value=a=b"]));
    }
    Connection::open(&fixture.state.database_path).unwrap().execute("UPDATE users SET display_name='更新后的姓名' WHERE user_id='u'", []).unwrap();
    assert_eq!(
        record["data"]["sha256"],
        format!("{:x}", Sha256::digest(bytes))
    );
    assert_eq!(request().await.0, StatusCode::CONFLICT);
    let (status, list) = fixture
        .request(
            "GET",
            "/api/platform-versions",
            None,
            true,
            "http://localhost:8778",
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&list).unwrap()["data"][0]["uploadedByName"], "更新后的姓名");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&list).unwrap()["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let id = record["data"]["packageId"].as_str().unwrap();
    let (status, download) = fixture
        .request(
            "GET",
            &format!("/api/platform-versions/{id}/download"),
            None,
            true,
            "http://localhost:8778",
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(download.as_ref(), bytes);
    assert_eq!(
        fixture
            .request(
                "POST",
                "/api/platform-versions",
                Some(&make_zip(true, None)),
                true,
                "http://localhost:8778"
            )
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(fs::read_dir(storage(&fixture.state)).unwrap().count(), 1);
    let db = Connection::open(&fixture.state.database_path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM platform_package_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn offline_upload_exceeds_the_previous_global_body_limit() {
    let fixture = Fixture::new("platform_admin");
    let payload = vec![7u8; 12 * 1024 * 1024];
    let mut manifest = image_manifest();
    manifest["image"]["size"] = serde_json::json!(payload.len());
    manifest["image"]["sha256"] = serde_json::json!(format!("{:x}", Sha256::digest(&payload)));
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(manifest.to_string().as_bytes()).unwrap();
    zip.start_file("image.tar", options).unwrap();
    zip.write_all(&payload).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let (status, response) = fixture
        .request(
            "POST",
            "/api/platform-versions",
            Some(&bytes),
            true,
            "http://localhost:8778",
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&response)
    );
}

#[tokio::test]
async fn enforces_auth_role_and_cross_site_upload_protection() {
    let admin = Fixture::new("platform_admin");
    let operator = Fixture::new("platform_operator");
    assert_eq!(
        admin
            .request("GET", "/api/platform-versions", None, false, "")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        operator
            .request("GET", "/api/platform-versions", None, true, "")
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        operator
            .request(
                "POST",
                "/api/platform-versions",
                Some(&make_zip(false, None)),
                true,
                "http://localhost:8778"
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        admin
            .request(
                "POST",
                "/api/platform-versions",
                Some(&make_zip(false, None)),
                true,
                "https://untrusted.example"
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}
