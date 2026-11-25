use super::application_commerce::*;
use super::*;
use serde_json::{Value, json};
struct Fixture {
    state: AppState,
    buyer: HeaderMap,
    publisher: HeaderMap,
    other: HeaderMap,
    staff: HeaderMap,
    root: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!("app-commerce-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("db.sqlite");
    initialize_database(
        &path,
        &root.join("missing"),
        &root.join("missing"),
        "admin",
        Some("isolated-test-password"),
    )
    .unwrap();
    ops::initialize(&path).unwrap();
    let c = Connection::open(&path).unwrap();
    for subject in ["publisher", "buyer", "other"] {
        c.execute(
            "INSERT INTO customers(customer_id,name,created_at) VALUES(?,?,1)",
            params![subject, subject],
        )
        .unwrap();
    }
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
    let headers = |subject: &str| {
        let token=jsonwebtoken::encode(&Header::new(Algorithm::RS256),&json!({"licenseId":format!("license-{subject}"),"subject":subject,"modules":["platform"],"iat":Utc::now().timestamp(),"exp":Utc::now().timestamp()+3600,"iss":"yaya-operation-center","aud":"yaya-low-code"}),&state.signing_key).unwrap();
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        h
    };
    let publisher = headers("publisher");
    let buyer = headers("buyer");
    let other = headers("other");
    let (token, _) = staff_sessions::login(
        &c,
        LoginRequest {
            username: "admin".into(),
            password: "isolated-test-password".into(),
        },
        Utc::now().timestamp_millis(),
    )
    .unwrap();
    let mut staff = HeaderMap::new();
    staff.insert(
        "cookie",
        HeaderValue::from_str(&format!("license_center_session={token}")).unwrap(),
    );
    Fixture {
        state,
        buyer,
        publisher,
        other,
        staff,
        root,
    }
}
fn config(price: i64, rate: i64) -> Value {
    json!({"pricingMode":if price==0{"free"}else{"paid"},"priceCents":price,"publisherRateBasisPoints":rate,"currency":"CNY","licenseModel":"one_time","description":"整包应用"})
}
fn product(f: &Fixture, name: &str, price: i64, rate: i64) -> String {
    let c = Connection::open(&f.state.database_path).unwrap();
    let release = format!("release_{name}");
    c.execute("INSERT INTO application_products(product_id,publisher_subject,name) VALUES(?,'publisher',?)",params![name,name]).unwrap();
    c.execute("INSERT INTO application_releases(submission_id,app_id,app_name,version,status,submitted_by,applicant_subject,submitted_at,snapshot_json,product_id) VALUES(?,?,?,'1.0.0','approved','test','publisher',1,'{}',?)",params![release,name,name,name]).unwrap();
    c.execute("INSERT INTO application_listing_versions(product_id,release_id,listing_status) VALUES(?,?,'listed')",params![name,release]).unwrap();
    application_commerce::receive_config(&c, name, &release, "publisher", &config(price, rate))
        .unwrap();
    application_commerce::review_release(&c, &release, true, "admin", "").unwrap();
    application_commerce::select_listing(&c, name, &release, None).unwrap()
}
async fn buy(f: &Fixture, product: &str, choice: &str, key: &str) -> Value {
    checkout(
        State(f.state.clone()),
        f.buyer.clone(),
        Path(product.into()),
        Json(serde_json::from_value(json!({"commercialId":choice,"idempotencyKey":key})).unwrap()),
    )
    .await
    .unwrap()
    .0
    .data
    .unwrap()
}
fn count(f: &Fixture, table: &str) -> i64 {
    Connection::open(&f.state.database_path)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[tokio::test]
async fn free_claim_is_idempotent_and_not_a_manual_grant_or_revenue() {
    let f = fixture();
    let choice = product(&f, "free", 0, 0);
    let first = buy(&f, "free", &choice, "one").await;
    let again = buy(&f, "free", &choice, "one").await;
    assert_eq!(first["orderId"], again["orderId"]);
    assert_eq!(first["status"], "completed");
    assert_eq!(first["owned"], true);
    assert_eq!(count(&f, "orders"), 1);
    assert_eq!(count(&f, "transactions"), 0);
    assert_eq!(count(&f, "application_revenue_entries"), 0);
    assert!(
        checkout(
            State(f.state.clone()),
            f.buyer.clone(),
            Path("free".into()),
            Json(
                serde_json::from_value(json!({"commercialId":choice,"idempotencyKey":"two"}))
                    .unwrap()
            )
        )
        .await
        .is_err()
    );
    assert!(
        my_orders(State(f.state.clone()), HeaderMap::new())
            .await
            .is_err()
    );
    assert!(
        my_orders(State(f.state.clone()), f.other.clone())
            .await
            .unwrap()
            .0
            .data
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn paid_order_freezes_price_and_share_and_duplicate_payment_never_reissues() {
    let f = fixture();
    let choice = product(&f, "paid", 19901, 7500);
    let first = buy(&f, "paid", &choice, "one").await;
    let duplicate = buy(&f, "paid", &choice, "two").await;
    assert_eq!(first["orderId"], duplicate["orderId"]);
    assert_eq!(first["owned"], false);
    assert_eq!(count(&f, "application_entitlement_sources"), 0);
    let c = Connection::open(&f.state.database_path).unwrap();
    application_commerce::receive_config(
        &c,
        "paid",
        "release_paid",
        "publisher",
        &config(50000, 5000),
    )
    .unwrap();
    // Technical approval must not approve a later standalone pricing revision.
    application_commerce::review_release(&c, "release_paid", true, "admin", "").unwrap();
    let latest:String=c.query_row("SELECT id FROM application_commercial_versions WHERE product_id='paid' ORDER BY rowid DESC LIMIT 1",[],|r|r.get(0)).unwrap();
    assert!(select_listing(&c, "paid", "release_paid", Some(&latest)).is_err());
    let _ = review_commercial(
        State(f.state.clone()),
        f.staff.clone(),
        Path(latest),
        Json(json!({"approved":true})),
    )
    .await
    .unwrap();
    application_commerce::select_listing(&c, "paid", "release_paid", None).unwrap();
    let order = first["orderId"].as_str().unwrap();
    assert!(
        simulate_payment(State(f.state.clone()), f.other.clone(), Path(order.into()))
            .await
            .is_err()
    );
    for _ in 0..2 {
        let result = simulate_payment(State(f.state.clone()), f.buyer.clone(), Path(order.into()))
            .await
            .unwrap()
            .0
            .data
            .unwrap();
        assert_eq!(result["owned"], true);
        assert_eq!(result["amountCents"], 19901);
        assert!(result["snapshot"].get("publisherRateBasisPoints").is_none());
    }
    assert_eq!(count(&f, "transactions"), 1);
    assert_eq!(count(&f, "application_entitlement_sources"), 1);
    assert_eq!(count(&f, "application_revenue_entries"), 1);
    let (gross, share, platform): (i64, i64, i64) = c
        .query_row(
            "SELECT gross_cents,amount_cents,platform_cents FROM application_revenue_entries",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((gross, share, platform), (19901, 14925, 4976));
}
#[tokio::test]
async fn refund_preserves_gifts_and_posts_a_single_reversal_even_after_settlement() {
    let f = fixture();
    let choice = product(&f, "paid", 10000, 8000);
    let info = buy(&f, "paid", &choice, "key").await;
    let order = info["orderId"].as_str().unwrap();
    let _ = simulate_payment(State(f.state.clone()), f.buyer.clone(), Path(order.into()))
        .await
        .unwrap();
    let _ = settle(
        State(f.state.clone()),
        f.staff.clone(),
        Json(json!({"publisherSubject":"publisher"})),
    )
    .await
    .unwrap();
    let c = Connection::open(&f.state.database_path).unwrap();
    application_commerce::manual_grant(&c, "buyer", "paid", None).unwrap();
    let _ = order_action(
        State(f.state.clone()),
        f.buyer.clone(),
        Path(order.into()),
        Json(json!({"action":"refund","reason":"模拟退款"})),
    )
    .await
    .unwrap();
    for _ in 0..2 {
        let _ = refund(
            State(f.state.clone()),
            f.staff.clone(),
            Path(order.into()),
            Json(json!({"approved":true})),
        )
        .await
        .unwrap();
    }
    assert_eq!(count(&f, "transactions"), 2);
    assert_eq!(count(&f, "application_revenue_entries"), 2);
    assert!(application_commerce::entitled(&c, "buyer", "paid").unwrap());
    let sum: i64 = c
        .query_row(
            "SELECT SUM(amount_cents) FROM application_revenue_entries",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sum, 0);
    assert!(
        settle(
            State(f.state.clone()),
            f.staff.clone(),
            Json(json!({"publisherSubject":"publisher"}))
        )
        .await
        .is_err()
    );
    application_commerce::revoke(&c, "buyer", "paid").unwrap();
    assert!(!application_commerce::entitled(&c, "buyer", "paid").unwrap());
}
#[tokio::test]
async fn closed_orders_and_stale_prices_cannot_pay_and_invalid_configs_do_not_publish() {
    let f = fixture();
    let choice = product(&f, "paid", 100, 5000);
    let stale = checkout(
        State(f.state.clone()),
        f.buyer.clone(),
        Path("paid".into()),
        Json(
            serde_json::from_value(json!({"commercialId":"old","idempotencyKey":"stale"})).unwrap(),
        ),
    )
    .await;
    assert!(stale.is_err());
    assert_eq!(count(&f, "orders"), 0);
    let info = buy(&f, "paid", &choice, "key").await;
    let order = info["orderId"].as_str().unwrap();
    let _ = order_action(
        State(f.state.clone()),
        f.buyer.clone(),
        Path(order.into()),
        Json(json!({"action":"cancel"})),
    )
    .await
    .unwrap();
    assert!(
        simulate_payment(State(f.state.clone()), f.buyer.clone(), Path(order.into()))
            .await
            .is_err()
    );
    let c = Connection::open(&f.state.database_path).unwrap();
    for bad in [config(-1, 5000), config(0, 5000), config(100, 10001)] {
        assert!(
            application_commerce::receive_config(&c, "paid", "release_paid", "publisher", &bad)
                .is_err()
        );
    }
    let preview = application_market_reset::command(&f.state.database_path, None).unwrap();
    assert_eq!(preview["preview"]["resetBlocked"], true);
    assert!(
        application_market_reset::command(&f.state.database_path, preview["confirmation"].as_str())
            .is_err()
    );
}
#[tokio::test]
async fn payment_failure_rolls_back_money_entitlements_and_revenue() {
    let f = fixture();
    let choice = product(&f, "paid", 100, 7000);
    let info = buy(&f, "paid", &choice, "key").await;
    let c = Connection::open(&f.state.database_path).unwrap();
    c.execute_batch("CREATE TRIGGER reject_entitlement BEFORE INSERT ON application_entitlement_sources BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert!(
        simulate_payment(
            State(f.state.clone()),
            f.buyer.clone(),
            Path(info["orderId"].as_str().unwrap().into())
        )
        .await
        .is_err()
    );
    assert_eq!(count(&f, "transactions"), 0);
    assert_eq!(count(&f, "application_revenue_entries"), 0);
    assert_eq!(count(&f, "application_entitlement_sources"), 0);
    let status: String = c
        .query_row("SELECT status FROM application_order_details", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(status, "pending_payment");
}

#[tokio::test]
async fn publisher_changes_require_ownership_and_explicit_review_and_listing() {
    let f = fixture();
    let original = product(&f, "paid", 10000, 8000);
    for headers in [f.buyer.clone(), f.other.clone()] {
        assert!(
            list_commercial(State(f.state.clone()), headers.clone(), Path("paid".into()))
                .await
                .is_err()
        );
        assert!(
            submit_commercial(
                State(f.state.clone()),
                headers,
                Path("paid".into()),
                Json(json!({"releaseId":"release_paid","config":config(20000,7000)}))
            )
            .await
            .is_err()
        );
    }
    let _ = submit_commercial(
        State(f.state.clone()),
        f.publisher.clone(),
        Path("paid".into()),
        Json(json!({"releaseId":"release_paid","config":config(20000,7000)})),
    )
    .await
    .unwrap();
    let versions = list_commercial(
        State(f.state.clone()),
        f.publisher.clone(),
        Path("paid".into()),
    )
    .await
    .unwrap()
    .0
    .data
    .unwrap();
    let latest = versions[0]["id"].as_str().unwrap();
    let c = Connection::open(&f.state.database_path).unwrap();
    assert!(select_listing(&c, "paid", "release_paid", Some(latest)).is_err());
    let _ = review_commercial(
        State(f.state.clone()),
        f.staff.clone(),
        Path(latest.into()),
        Json(json!({"approved":true})),
    )
    .await
    .unwrap();
    let selected: String = c
        .query_row(
            "SELECT commercial_id FROM application_sales_listing WHERE product_id='paid'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(selected, original);
    assert_eq!(
        buy(&f, "paid", &original, "still-original").await["amountCents"],
        10000
    );
    select_listing(&c, "paid", "release_paid", Some(latest)).unwrap();
    assert!(review_release(&c, "missing-release", true, "admin", "").is_err());
    assert!(select_listing(&c, "paid", "missing-release", None).is_err());
}
#[tokio::test]
async fn refund_freezes_unsettled_revenue_and_rejection_restores_it() {
    let f = fixture();
    let choice = product(&f, "paid", 10000, 8000);
    let info = buy(&f, "paid", &choice, "key").await;
    let order = info["orderId"].as_str().unwrap();
    let _ = simulate_payment(State(f.state.clone()), f.buyer.clone(), Path(order.into()))
        .await
        .unwrap();
    let _ = order_action(
        State(f.state.clone()),
        f.buyer.clone(),
        Path(order.into()),
        Json(json!({"action":"refund","reason":"模拟退款"})),
    )
    .await
    .unwrap();
    assert!(
        settle(
            State(f.state.clone()),
            f.staff.clone(),
            Json(json!({"publisherSubject":"publisher"}))
        )
        .await
        .is_err()
    );
    let c = Connection::open(&f.state.database_path).unwrap();
    let state: String = c
        .query_row("SELECT status FROM application_revenue_entries", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(state, "frozen");
    let _ = refund(
        State(f.state.clone()),
        f.staff.clone(),
        Path(order.into()),
        Json(json!({"approved":false,"reason":"保留订单"})),
    )
    .await
    .unwrap();
    assert!(entitled(&c, "buyer", "paid").unwrap());
    let _ = settle(
        State(f.state.clone()),
        f.staff.clone(),
        Json(json!({"publisherSubject":"publisher"})),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn withdrawn_products_remain_visible_only_to_owners_and_publishers() {
    let f = fixture();
    let choice = product(&f, "paid", 10000, 8000);
    let info = buy(&f, "paid", &choice, "key").await;
    let _ = simulate_payment(
        State(f.state.clone()),
        f.buyer.clone(),
        Path(info["orderId"].as_str().unwrap().into()),
    )
    .await
    .unwrap();
    let c = Connection::open(&f.state.database_path).unwrap();
    let item = json!({"productId":"paid","publisherSubject":"publisher","releaseId":"release_paid","listingStatus":"withdrawn","dependencies":[{"internal":"private"}]});
    for (subject, expected) in [("buyer", 1), ("other", 0), ("publisher", 1)] {
        let mut items = vec![item.clone()];
        catalogue(&c, &mut items, Some(subject), true).unwrap();
        assert_eq!(items.len(), expected);
        if let Some(item) = items.first() {
            assert!(item.get("publisherRateBasisPoints").is_none());
            assert!(item.get("dependencies").is_none());
        }
    }
}
