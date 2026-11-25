use super::*;
use axum::{
    Router,
    routing::{delete, get, post},
};
use serde_json::{Value, json};

#[tokio::test]
async fn application_market_v3_http_lifecycle() {
    let root = std::env::temp_dir().join(format!("market-v3-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let packages = root.join("packages");
    // This test runs in its own filtered process; no development database or package path is used.
    unsafe {
        std::env::set_var("YAYA_APPLICATION_PACKAGES_PATH", &packages);
    }
    let path = root.join("market.sqlite");
    let missing = root.join("missing");
    initialize_database(
        &path,
        &missing,
        &missing,
        "admin",
        Some("isolated-test-password"),
    )
    .unwrap();
    initialize_database(
        &path,
        &missing,
        &missing,
        "admin",
        Some("isolated-test-password"),
    )
    .unwrap();
    ops::initialize(&path).unwrap();
    let c = Connection::open(&path).unwrap();
    let (session, _) = staff_sessions::login(
        &c,
        LoginRequest {
            username: "admin".into(),
            password: "isolated-test-password".into(),
        },
        Utc::now().timestamp_millis(),
    )
    .unwrap();
    let state = AppState {
        signing_key: Arc::new(
            EncodingKey::from_rsa_pem(include_bytes!(
                "../tests/fixtures/licensing-test-private.pem"
            ))
            .unwrap(),
        ),
        verification_key: Arc::new(
            DecodingKey::from_rsa_pem(include_bytes!(
                "../tests/fixtures/licensing-test-public.pem"
            ))
            .unwrap(),
        ),
        database_path: path.clone(),
        sessions: Arc::new(RwLock::new(HashMap::new())),
        ops: Arc::new(ops::OpsState::new(false, root.join("none"))),
    };
    let license = json!({"licenseId":"test-license","subject":"publisher","modules":["platform"],"iat":Utc::now().timestamp(),"exp":Utc::now().timestamp()+3600,"iss":"yaya-operation-center","aud":"yaya-low-code"});
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(Algorithm::RS256),
        &license,
        &state.signing_key,
    )
    .unwrap();
    let router = Router::new()
        .route("/publisher-history", get(publisher_application_submissions))
        .route("/submit", post(receive_application_submission))
        .route("/import", post(import_application_package))
        .route("/review/{id}", post(review_application_submission))
        .route("/listing/{id}", post(set_application_listing))
        .route("/delete/{id}", delete(delete_application_product))
        .route("/products", get(list_public_applications))
        .route("/grant/{id}", post(grant_application_entitlement))
        .route("/ticket/{id}", post(create_application_download_ticket))
        .route("/checkout/{id}", post(application_commerce::checkout))
        .route("/pay/{id}", post(application_commerce::simulate_payment))
        .route("/download/{id}", get(download_application_package))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let bytes = if let Ok(path) = std::env::var("MARKET_TEST_EXPORT_PATH") {
        fs::read(path).unwrap()
    } else {
        application_protocol::tests::pack(&application_protocol::tests::bundle(), "1.0.0")
    };
    let (_, bundle) = application_protocol::read(&bytes).unwrap();
    let metadata = json!({"submissionId":"sub_market_test","appId":bundle["members"][0]["appId"],"appName":bundle["packageName"].as_str().unwrap_or("套应用测试"),"version":"1.0.0","status":"approved","submittedBy":"test","submittedAt":1,"snapshot":{"commercial":{"pricingMode":"paid","priceCents":19900,"currency":"CNY","licenseModel":"one_time","publisherRateBasisPoints":8000,"description":"完整套应用"}}});
    Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO customers(customer_id,name,created_at) VALUES('publisher','发布方',1)",
            [],
        )
        .unwrap();
    Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO customers(customer_id,name,created_at) VALUES('buyer','购买客户',1)",
            [],
        )
        .unwrap();
    let mut buyer_license = license.clone();
    buyer_license["subject"] = json!("buyer");
    let buyer_token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(Algorithm::RS256),
        &buyer_license,
        &state.signing_key,
    )
    .unwrap();
    let client = reqwest::Client::new();
    let upload = |data: Vec<u8>, meta: Value| {
        let mut body=format!("--market-boundary\r\nContent-Disposition: form-data; name=\"metadata\"\r\n\r\n{meta}\r\n--market-boundary\r\nContent-Disposition: form-data; name=\"package\"; filename=\"application.zip\"\r\nContent-Type: application/zip\r\n\r\n").into_bytes();
        body.extend(data);
        body.extend(b"\r\n--market-boundary--\r\n");
        client
            .post(format!("{url}/submit"))
            .bearer_auth(&token)
            .header(
                "content-type",
                "multipart/form-data; boundary=market-boundary",
            )
            .body(body)
            .send()
    };
    let response = upload(bytes.clone(), metadata.clone()).await.unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    let repeated: Value = upload(bytes.clone(), metadata.clone())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(repeated["data"]["status"], "pending_review");
    let mut changed_metadata = metadata.clone();
    changed_metadata["appName"] = json!("changed");
    assert_eq!(
        upload(bytes.clone(), changed_metadata)
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    let product = repeated["data"]["productId"].as_str().unwrap();
    assert_ne!(product, "prod_source-app-1");
    let mut headers = HeaderMap::new();
    headers.insert(
        "cookie",
        HeaderValue::from_str(&format!("license_center_session={session}")).unwrap(),
    );
    let review = review_application_submission(
        State(state.clone()),
        headers.clone(),
        Path("sub_market_test".into()),
        Json(ReviewApplicationSubmissionRequest {
            approved: true,
            reason: String::new(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(review.0.data.unwrap()["status"], "approved");
    let catalogue: Value = client
        .get(format!("{url}/products"))
        .bearer_auth(&buyer_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(catalogue["data"], json!([]));
    let mut altered = bundle.clone();
    altered["members"][0]["resources"]["apps"][0]["name"] = json!("tampered");
    assert_eq!(
        upload(
            application_protocol::tests::pack(&altered, "1.0.0"),
            metadata.clone()
        )
        .await
        .unwrap()
        .status(),
        StatusCode::CONFLICT
    );
    let _ = set_application_listing(
        State(state.clone()),
        headers.clone(),
        Path(product.into()),
        Json(json!({"listed":true,"releaseId":"sub_market_test"})),
    )
    .await
    .unwrap();
    let denied = client
        .post(format!("{url}/ticket/{product}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let catalog: Value = client
        .get(format!("{url}/products"))
        .bearer_auth(&buyer_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let commercial_id = catalog["data"][0]["commercialId"].as_str().unwrap();
    assert!(catalog["data"][0].get("publisherRateBasisPoints").is_none());
    let purchase: Value = client
        .post(format!("{url}/checkout/{product}"))
        .bearer_auth(&buyer_token)
        .json(&json!({"commercialId":commercial_id,"idempotencyKey":"cross-platform-purchase"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(purchase["data"]["status"], "pending_payment");
    let paid: Value = client
        .post(format!(
            "{url}/pay/{}",
            purchase["data"]["orderId"].as_str().unwrap()
        ))
        .bearer_auth(&buyer_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(paid["data"]["owned"], true);
    let ticket: Value = client
        .post(format!("{url}/ticket/{product}"))
        .bearer_auth(&buyer_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(ticket["data"]["releaseId"], "sub_market_test");
    assert_eq!(
        ticket["data"]["packageSha256"],
        application_protocol::digest(&bytes)
    );
    let download_url = format!(
        "{url}/download/{}",
        ticket["data"]["ticket"].as_str().unwrap()
    );
    let (one, two) = tokio::join!(
        client.get(&download_url).send(),
        client.get(&download_url).send()
    );
    let one = one.unwrap();
    let two = two.unwrap();
    assert_ne!(one.status().is_success(), two.status().is_success());
    let downloaded = if one.status().is_success() {
        one.bytes().await.unwrap()
    } else {
        two.bytes().await.unwrap()
    };
    assert_eq!(downloaded.as_ref(), bytes.as_slice());
    if let Ok(path) = std::env::var("MARKET_TEST_DOWNLOADED_PATH") {
        fs::write(path, downloaded).unwrap();
    }
    // A rejected member-removal release cannot mutate the existing product or version.
    let mut smaller = bundle.clone();
    smaller["members"].as_array_mut().unwrap().pop();
    assert!(application_protocol::resources(&smaller).is_err());
    c.execute("INSERT INTO customers(customer_id,name,created_at) VALUES('offline-customer','离线发布方',1)",[]).unwrap();
    let offline_config=json!({"pricingMode":"free","priceCents":0,"currency":"CNY","licenseModel":"one_time","publisherRateBasisPoints":0,"description":"离线应用"}).to_string();
    let offline_upload = |data: Vec<u8>,
                          publisher: &str,
                          product: Option<&str>,
                          authenticated: bool| {
        let mut body=format!("--offline-boundary\r\nContent-Disposition: form-data; name=\"publisherSubject\"\r\n\r\n{publisher}\r\n--offline-boundary\r\nContent-Disposition: form-data; name=\"productId\"\r\n\r\n{}\r\n--offline-boundary\r\nContent-Disposition: form-data; name=\"package\"; filename=\"suite.zip\"\r\nContent-Type: application/zip\r\n\r\n",product.unwrap_or("")).into_bytes();
        body.extend(data);
        body.extend(format!("\r\n--offline-boundary\r\nContent-Disposition: form-data; name=\"commercial\"\r\n\r\n{offline_config}").as_bytes());
        body.extend(b"\r\n--offline-boundary--\r\n");
        let request = client
            .post(format!("{url}/import"))
            .header(
                "content-type",
                "multipart/form-data; boundary=offline-boundary",
            )
            .body(body);
        if authenticated {
            request.header("cookie", format!("license_center_session={session}"))
        } else {
            request
        }
    };
    assert_eq!(
        offline_upload(bytes.clone(), "offline-customer", None, false)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        offline_upload(bytes.clone(), "missing-customer", None, true)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        offline_upload(b"not zip".to_vec(), "offline-customer", None, true)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let response = offline_upload(bytes.clone(), "offline-customer", None, true)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    let imported: Value = response.json().await.unwrap();
    assert_eq!(imported["data"]["status"], "pending_review");
    assert_eq!(imported["data"]["applicantSubject"], "offline-customer");
    let repeated: Value = offline_upload(bytes.clone(), "offline-customer", None, true)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        imported["data"]["submissionId"],
        repeated["data"]["submissionId"]
    );
    let offline_product = imported["data"]["productId"].as_str().unwrap();
    assert_eq!(imported["data"]["snapshot"]["submissionType"], "new_listing");
    assert_eq!(imported["data"]["snapshot"]["source"], "offline-import");
    let history: Value = client.get(format!("{url}/publisher-history")).bearer_auth(&token).send().await.unwrap().json().await.unwrap();
    let history = history["data"].as_array().unwrap();
    assert!(history.iter().any(|v|v["submissionId"] == "sub_market_test"));
    assert!(history.iter().all(|v|v["applicantSubject"] == "publisher"));
    assert!(!history.iter().any(|v|v["submissionId"] == imported["data"]["submissionId"]));
    let mut typed_bundle = bundle.clone();
    typed_bundle["publication"] = json!({"submissionType":"version_upgrade","targetProductId":offline_product});
    let typed_bytes = application_protocol::tests::pack(&typed_bundle,"9.0.0");
    assert_eq!(offline_upload(typed_bytes, "offline-customer", None, true).send().await.unwrap().status(), StatusCode::BAD_REQUEST);

    assert_eq!(
        offline_upload(
            bytes.clone(),
            "offline-customer",
            Some(offline_product),
            true
        )
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        offline_upload(bytes.clone(), "offline-customer", Some(product), true)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let next = application_protocol::tests::pack(&bundle, "1.0.1");
    let response = offline_upload(next, "offline-customer", Some(offline_product), true)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    let listing: String = c
        .query_row(
            "SELECT listing_status FROM application_listing_versions WHERE product_id=?",
            [offline_product],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(listing, "withdrawn");
    let _ = review_application_submission(
        State(state.clone()),
        headers.clone(),
        Path(imported["data"]["submissionId"].as_str().unwrap().into()),
        Json(ReviewApplicationSubmissionRequest {
            approved: true,
            reason: String::new(),
        }),
    )
    .await
    .unwrap();
    let _ = set_application_listing(
        State(state.clone()),
        headers.clone(),
        Path(offline_product.into()),
        Json(json!({"listed":true,"releaseId":imported["data"]["submissionId"]})),
    )
    .await
    .unwrap();
    let _ = grant_application_entitlement(
        State(state.clone()),
        headers.clone(),
        Path(offline_product.into()),
        Json(GrantApplicationEntitlementRequest {
            subject: "publisher".into(),
            reason: "隔离测试赠送".into(),
            expires_at: None,
        }),
    )
    .await
    .unwrap();
    let ticket: Value = client
        .post(format!("{url}/ticket/{offline_product}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        ticket["data"]["releaseId"],
        imported["data"]["submissionId"]
    );
    let response = client
        .get(format!(
            "{url}/download/{}",
            ticket["data"]["ticket"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.bytes().await.unwrap().as_ref(), bytes.as_slice());
    // Official publishing needs no customer record and remains purchasable after approval.
    let official_response = offline_upload(bytes.clone(), OFFICIAL_PUBLISHER, None, true).send().await.unwrap();
    assert!(official_response.status().is_success());
    let official: Value = official_response.json().await.unwrap();
    let official_id = official["data"]["productId"].as_str().unwrap();
    let official_release = official["data"]["submissionId"].as_str().unwrap();
    assert_eq!(official["data"]["applicantSubject"], OFFICIAL_PUBLISHER);
    let _ = review_application_submission(State(state.clone()), headers.clone(), Path(official_release.into()), Json(ReviewApplicationSubmissionRequest { approved: true, reason: String::new() })).await.unwrap();
    let _ = set_application_listing(State(state.clone()), headers.clone(), Path(official_id.into()), Json(json!({"listed":true,"releaseId":official_release}))).await.unwrap();
    let catalog: Value = client.get(format!("{url}/products")).bearer_auth(&buyer_token).send().await.unwrap().json().await.unwrap();
    let official_product = catalog["data"].as_array().unwrap().iter().find(|item|item["productId"] == official_id).unwrap();
    let purchase = client.post(format!("{url}/checkout/{official_id}")).bearer_auth(&buyer_token).json(&json!({"commercialId":official_product["commercialId"],"idempotencyKey":"official-free"})).send().await.unwrap();
    assert!(purchase.status().is_success(), "{}", purchase.text().await.unwrap());
    let upgrade = offline_upload(application_protocol::tests::pack(&bundle, "2.0.0"), OFFICIAL_PUBLISHER, Some(official_id), true).send().await.unwrap();
    assert!(upgrade.status().is_success());
    // Deleting requires withdrawal, admin auth, and preserves immutable history and commerce.
    assert_eq!(client.delete(format!("{url}/delete/{product}")).send().await.unwrap().status(), StatusCode::UNAUTHORIZED);
    let deleted = delete_application_product(State(state.clone()), headers.clone(), Path(product.into())).await;
    assert_eq!(deleted.unwrap_err().0, StatusCode::CONFLICT);
    let _ = set_application_listing(State(state.clone()), headers.clone(), Path(product.into()), Json(json!({"listed":false}))).await.unwrap();
    let releases_before: i64 = c.query_row("SELECT COUNT(*) FROM application_releases WHERE product_id=?", [product], |r|r.get(0)).unwrap();
    let orders_before: i64 = c.query_row("SELECT COUNT(*) FROM application_order_details", [], |r|r.get(0)).unwrap();
    let _ = delete_application_product(State(state.clone()), headers.clone(), Path(product.into())).await.unwrap();
    let Json(catalogue) = list_application_products(State(state.clone()), headers.clone()).await.unwrap();
    assert!(!catalogue.data.unwrap().iter().any(|item|item["productId"] == product));
    assert_eq!(c.query_row::<String,_,_>("SELECT status FROM application_products WHERE product_id=?", [product], |r|r.get(0)).unwrap(), "deleted");
    assert_eq!(c.query_row::<i64,_,_>("SELECT COUNT(*) FROM application_releases WHERE product_id=?", [product], |r|r.get(0)).unwrap(), releases_before);
    assert_eq!(c.query_row::<i64,_,_>("SELECT COUNT(*) FROM application_order_details", [], |r|r.get(0)).unwrap(), orders_before);
    assert_eq!(set_application_listing(State(state.clone()), headers.clone(), Path(product.into()), Json(json!({"listed":true,"releaseId":"sub_market_test"}))).await.unwrap_err().0, StatusCode::NOT_FOUND);
    assert_eq!(delete_application_product(State(state.clone()), headers.clone(), Path(product.into())).await.unwrap_err().0, StatusCode::NOT_FOUND);
    assert_eq!(offline_upload(application_protocol::tests::pack(&bundle, "9.0.0"), "publisher", Some(product), true).send().await.unwrap().status(), StatusCode::NOT_FOUND);
    // Commerce data is retained: reset is previewable but execution must be refused.
    let protected_preview = application_market_reset::command(&path, None).unwrap();
    assert_eq!(protected_preview["preview"]["resetBlocked"], true);
    assert!(
        application_market_reset::command(&path, protected_preview["confirmation"].as_str())
            .is_err()
    );
    // Exercise the legacy no-trade reset contract in a separate throwaway database copy.
    let legacy_path = root.join("legacy-reset.sqlite");
    c.execute("VACUUM INTO ?", [legacy_path.to_string_lossy().as_ref()])
        .unwrap();
    let c = Connection::open(&legacy_path).unwrap();
    c.execute_batch("DELETE FROM application_order_details;DELETE FROM application_revenue_entries;DELETE FROM application_settlements;DELETE FROM transactions;DELETE FROM orders;").unwrap();
    let path = legacy_path;
    c.execute("INSERT INTO ai_employee_products(id,title,price_cents,created_at,updated_at) VALUES('keep-employee','Keep',0,1,1)",[]).unwrap();
    let preview = application_market_reset::command(&path, None).unwrap();
    assert_eq!(preview["executed"], false);
    let count: i64 = c
        .query_row("SELECT COUNT(*) FROM application_products", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 3);
    c.execute("UPDATE application_products SET name='changed'", [])
        .unwrap();
    assert!(application_market_reset::command(&path, preview["confirmation"].as_str()).is_err());
    let preview = application_market_reset::command(&path, None).unwrap();
    application_market_reset::command(&path, preview["confirmation"].as_str()).unwrap();
    assert_eq!(
        c.query_row::<i64, _, _>("SELECT COUNT(*) FROM application_products", [], |r| r
            .get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM ai_employee_products WHERE id='keep-employee'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        1
    );
    server.abort();
    drop(c);
    drop(state);
    unsafe {
        std::env::remove_var("YAYA_APPLICATION_PACKAGES_PATH");
    }
    let _ = fs::remove_dir_all(root);
}
