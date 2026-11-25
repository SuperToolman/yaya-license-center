//! Only the customer row is deleted. Business dependencies must be cleared manually.
use super::*;

type Failure = (StatusCode, Json<Envelope<()>>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CustomerDeletionBlocker {
    kind: String,
    label: String,
    count: usize,
    record_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CustomerDeletionEligibility {
    can_delete: bool,
    blockers: Vec<CustomerDeletionBlocker>,
}

fn unavailable(_: rusqlite::Error) -> Failure {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "客户关联数据检查失败，请稍后重试",
    )
}

async fn require_customer_admin(headers: &HeaderMap, state: &AppState) -> Result<(), Failure> {
    let user = require_authenticated(headers, state).await?;
    if user.role != "platform_admin" {
        return Err(error(StatusCode::FORBIDDEN, "仅平台超级管理员可删除客户"));
    }
    Ok(())
}

fn check(
    connection: &Connection,
    customer_id: &str,
) -> Result<CustomerDeletionEligibility, Failure> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id = ?)",
            [customer_id],
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "客户不存在"));
    }
    // UNION deduplicates cycles (order <-> license) and records reached along multiple paths.
    // Audit logs are independent historical snapshots, not owned business records.
    let mut statement = connection.prepare(
        "WITH RECURSIVE edges(source_kind, source_id, target_kind, target_id) AS (
            SELECT 'customer', customer_id, 'order', order_id FROM orders
            UNION ALL SELECT 'customer', customer_id, 'transaction', transaction_id FROM transactions
            UNION ALL SELECT 'order', order_id, 'transaction', transaction_id FROM transactions
            UNION ALL SELECT 'transaction', t.transaction_id, 'order', o.order_id FROM transactions t JOIN orders o ON o.order_id = t.order_id
            UNION ALL SELECT 'customer', customer_id, 'subject', customer_id FROM customers
            UNION ALL SELECT 'order', order_id, 'license', license_id FROM licenses
            UNION ALL SELECT 'order', order_id, 'legacy_license', license_id FROM legacy_licenses
            UNION ALL SELECT 'order', o.order_id, 'license', l.license_id FROM orders o JOIN licenses l ON l.license_id = o.license_id
            UNION ALL SELECT 'order', o.order_id, 'legacy_license', l.license_id FROM orders o JOIN legacy_licenses l ON l.license_id = o.license_id
            UNION ALL SELECT 'license', l.license_id, 'order', o.order_id FROM licenses l JOIN orders o ON o.order_id = l.order_id
            UNION ALL SELECT 'legacy_license', l.license_id, 'order', o.order_id FROM legacy_licenses l JOIN orders o ON o.order_id = l.order_id
            UNION ALL SELECT 'subject', subject, 'license', license_id FROM licenses
            UNION ALL SELECT 'subject', subject, 'legacy_license', license_id FROM legacy_licenses
            UNION ALL SELECT 'license', license_id, 'subject', subject FROM licenses
            UNION ALL SELECT 'legacy_license', license_id, 'subject', subject FROM legacy_licenses
            UNION ALL SELECT 'order', order_id, 'commission', entry_id FROM commission_entries
            UNION ALL SELECT 'commission', e.entry_id, 'settlement', b.batch_id FROM commission_entries e JOIN settlement_batches b ON b.batch_id = e.settlement_batch_id
            UNION ALL SELECT 'subject', applicant_subject, 'release', submission_id FROM application_releases
            UNION ALL SELECT 'subject', publisher_subject, 'product', product_id FROM application_products
            UNION ALL SELECT 'product', p.product_id, 'release', r.submission_id FROM application_products p JOIN application_releases r ON r.product_id = p.product_id
            UNION ALL SELECT 'subject', subject, 'entitlement', subject || ':' || product_id FROM application_entitlements
            UNION ALL SELECT 'product', product_id, 'entitlement', subject || ':' || product_id FROM application_entitlements
            UNION ALL SELECT 'product', product_id, 'listing', product_id FROM application_listing_versions
            UNION ALL SELECT 'release', release_id, 'listing', product_id FROM application_listing_versions
            UNION ALL SELECT 'listing', l.product_id, 'release', r.submission_id FROM application_listing_versions l JOIN application_releases r ON r.submission_id = l.release_id
            UNION ALL SELECT 'subject', subject, 'application_order', order_id FROM application_order_details
            UNION ALL SELECT 'subject', publisher_subject, 'application_revenue', id FROM application_revenue_entries
            UNION ALL SELECT 'subject', publisher_subject, 'application_settlement', id FROM application_settlements
            UNION ALL SELECT 'subject', subject, 'entitlement_source', id FROM application_entitlement_sources
            UNION ALL SELECT 'product', product_id, 'commercial', id FROM application_commercial_versions
            UNION ALL SELECT 'subject', subject, 'download_ticket', ticket FROM application_download_tickets
            UNION ALL SELECT 'release', release_id, 'download_ticket', ticket FROM application_download_tickets
            UNION ALL SELECT 'customer', customer_id, 'website_session', token_hash FROM website_sessions
        ), related(kind, id) AS (
            SELECT 'customer', ?1
            UNION SELECT edges.target_kind, edges.target_id FROM related
                JOIN edges ON edges.source_kind = related.kind AND edges.source_id = related.id
                WHERE edges.target_id IS NOT NULL AND edges.target_id != ''
        )
        SELECT kind, id FROM related WHERE kind NOT IN ('customer', 'subject') ORDER BY kind, id"
    ).map_err(unavailable)?;
    let rows = statement
        .query_map([customer_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(unavailable)?;
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let (kind, id) = row.map_err(unavailable)?;
        groups.entry(kind).or_default().push(id);
    }
    let has_instances: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='platform_instances')", [], |r|r.get(0)).map_err(unavailable)?;
    if has_instances {
        let mut query = connection.prepare("SELECT instance_id FROM platform_instances WHERE customer_id=? ORDER BY instance_id").map_err(unavailable)?;
        let ids = query.query_map([customer_id], |r|r.get::<_,String>(0)).map_err(unavailable)?.collect::<rusqlite::Result<Vec<_>>>().map_err(unavailable)?;
        if !ids.is_empty() { groups.insert("platform_instance".into(),ids); }
    }
    let blockers = groups
        .into_iter()
        .map(|(kind, ids)| {
            let label = match kind.as_str() {
                "order" => "订单",
                "transaction" => "收款流水",
                "license" => "许可证",
                "legacy_license" => "历史许可证",
                "commission" => "提成与返利台账",
                "settlement" => "结算批次",
                "release" => "应用发布申请",
                "product" => "客户发布的应用产品",
                "listing" => "应用上架记录",
                "entitlement" => "应用授权",
                "download_ticket" => "应用下载凭证",
                "website_session" => "官网客户会话",
                "platform_instance" => "平台实例（含归档记录）",
                _ => "关联数据",
            }
            .to_string();
            let count = ids.len();
            // Authentication token hashes must not be returned to the browser.
            let record_ids = if kind == "website_session" || kind == "download_ticket" {
                vec![]
            } else {
                ids.into_iter().take(10).collect()
            };
            CustomerDeletionBlocker {
                kind,
                label,
                count,
                record_ids,
            }
        })
        .collect::<Vec<_>>();
    Ok(CustomerDeletionEligibility {
        can_delete: blockers.is_empty(),
        blockers,
    })
}

pub(super) async fn get_eligibility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(customer_id): Path<String>,
) -> Result<Json<Envelope<CustomerDeletionEligibility>>, Failure> {
    require_customer_admin(&headers, &state).await?;
    let mut connection = Connection::open(&state.database_path).map_err(unavailable)?;
    let transaction = connection.transaction().map_err(unavailable)?;
    let eligibility = check(&transaction, &customer_id)?;
    Ok(Json(success("客户删除条件已检查", eligibility)))
}

fn remove(
    connection: &mut Connection,
    customer_id: &str,
) -> Result<CustomerDeletionEligibility, Failure> {
    // Recheck under a write lock to prevent related data appearing between check and deletion.
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(unavailable)?;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let eligibility = check(&transaction, customer_id)?;
    if !eligibility.can_delete {
        let details = eligibility
            .blockers
            .iter()
            .map(|item| format!("{} {} 条", item.label, item.count))
            .collect::<Vec<_>>()
            .join("、");
        return Err(error(
            StatusCode::CONFLICT,
            &format!(
                "客户仍有关联数据：{details}。请管理员先手动清理关联数据及其下级关联数据，再删除客户。"
            ),
        ));
    }
    transaction
        .execute("DELETE FROM customers WHERE customer_id = ?", [customer_id])
        .map_err(unavailable)?;
    transaction.commit().map_err(unavailable)?;
    Ok(eligibility)
}

pub(super) async fn delete_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(customer_id): Path<String>,
) -> Result<Json<Envelope<CustomerDeletionEligibility>>, Failure> {
    require_customer_admin(&headers, &state).await?;
    let mut connection = Connection::open(&state.database_path).map_err(unavailable)?;
    let eligibility = remove(&mut connection, &customer_id)?;
    Ok(Json(success("客户已删除", eligibility)))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Database {
        root: PathBuf,
        connection: Connection,
    }

    impl Database {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("customer-deletion-{}", Uuid::new_v4()));
            let path = root.join("test.sqlite");
            let missing = root.join("missing.json");
            initialize_database(&path, &missing, &missing, "admin", Some("test-password")).unwrap();
            let connection = Connection::open(path).unwrap();
            website_auth::initialize(&connection).unwrap();
            connection.execute("INSERT INTO customers(customer_id,name,created_at) VALUES('c1','客户甲',1),('c2','客户乙',1)", []).unwrap();
            Self { root, connection }
        }
    }

    impl Drop for Database {
        fn drop(&mut self) {
            // The connection is closed before removing its directory on Windows.
            let connection =
                std::mem::replace(&mut self.connection, Connection::open_in_memory().unwrap());
            drop(connection);
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn platform_instances_block_customer_deletion_including_archived_drafts() {
        let mut db = Database::new();
        db.connection.execute_batch("CREATE TABLE platform_instances(instance_id TEXT PRIMARY KEY,customer_id TEXT,status TEXT); INSERT INTO platform_instances VALUES('instance1','c1','archived');").unwrap();
        let report = check(&db.connection, "c1").unwrap();
        assert!(!report.can_delete);
        assert_eq!(report.blockers[0].kind, "platform_instance");
        assert_eq!(report.blockers[0].record_ids, vec!["instance1"]);
        assert_eq!(remove(&mut db.connection,"c1").unwrap_err().0,StatusCode::CONFLICT);
        assert!(check(&db.connection,"c2").unwrap().can_delete);
    }

    #[test]
    fn deletes_only_unlinked_customer_and_preserves_audit_history() {
        let mut db = Database::new();
        db.connection.execute("INSERT INTO audit_logs(created_at,level,source,message,customer_id,customer_name) VALUES(1,'info','test','历史记录','c1','客户甲')", []).unwrap();
        assert!(check(&db.connection, "c1").unwrap().can_delete);
        assert!(remove(&mut db.connection, "c1").unwrap().can_delete);
        let customers: i64 = db
            .connection
            .query_row("SELECT COUNT(*) FROM customers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(customers, 1);
        assert!(check(&db.connection, "c2").unwrap().can_delete);
        assert_eq!(
            check(&db.connection, "c1").unwrap_err().0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            db.connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_logs WHERE customer_id='c1'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn reports_direct_and_nested_dependencies_without_duplicates_or_cascade() {
        let mut db = Database::new();
        db.connection.execute_batch(
            "INSERT INTO orders(order_id,order_no,customer_id,items_json,total_amount_cents,status,created_at,license_id) VALUES('o1','ORDER-1','c1','[]',0,'cancelled',1,'l1');
             INSERT INTO transactions(transaction_id,order_id,customer_id,amount_cents,method,occurred_at) VALUES('t1','o1','c1',100,'cash',1);
             INSERT INTO licenses(license_id,license,subject,modules_json,issued_at,expires_at,module_expires_at_json,platform_status,module_statuses_json,order_id) VALUES('l1','token','other-subject','[]',1,2,'{}','destroyed','{}','o1');
             INSERT INTO commission_entries(entry_id,order_id,provider_id,product_type,received_amount_cents,rate_basis_points,commission_amount_cents,created_at,settlement_batch_id) VALUES('e1','o1','p1','platform',100,100,1,1,'b1');
             INSERT INTO settlement_batches(batch_id,provider_id,total_amount_cents,entry_count,settled_at) VALUES('b1','p1',1,1,1);
             INSERT INTO application_entitlements(subject,product_id,created_at,updated_at,status) VALUES('other-subject','app1',1,1,'revoked');"
        ).unwrap();
        let report = check(&db.connection, "c1").unwrap();
        let kinds = report
            .blockers
            .iter()
            .map(|item| (item.kind.as_str(), item.count))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            kinds,
            BTreeMap::from([
                ("commission", 1),
                ("entitlement", 1),
                ("license", 1),
                ("order", 1),
                ("settlement", 1),
                ("transaction", 1)
            ])
        );
        assert!(check(&db.connection, "c2").unwrap().can_delete);
        let error = remove(&mut db.connection, "c1").unwrap_err();
        assert_eq!(error.0, StatusCode::CONFLICT);
        assert!(error.1.0.message.contains("请管理员先手动清理"));
        assert_eq!(check(&db.connection, "c1").unwrap().blockers.len(), 6);
    }

    #[test]
    fn checks_subject_owned_apps_their_children_and_sessions() {
        let db = Database::new();
        db.connection.execute_batch(
            "INSERT INTO application_products(product_id,publisher_subject,name) VALUES('app1','c1','应用');
             INSERT INTO application_releases(submission_id,app_id,app_name,submitted_by,applicant_subject,submitted_at,snapshot_json) VALUES('r1','low-code-1','应用','test','c1',1,'{}');
             INSERT INTO application_listing_versions(product_id,release_id) VALUES('app1','r1');
             INSERT INTO application_entitlements(subject,product_id,created_at,updated_at) VALUES('c2','app1',1,1);
             INSERT INTO application_download_tickets(ticket,release_id,subject,expires_at) VALUES('secret-ticket','r1','c2',1);
             INSERT INTO website_sessions(token_hash,customer_id,mobile,expires_at,created_at) VALUES('secret-hash','c1','13800000000',1,1);"
        ).unwrap();
        let report = check(&db.connection, "c1").unwrap();
        assert!(!report.can_delete);
        for kind in [
            "product",
            "release",
            "listing",
            "entitlement",
            "download_ticket",
            "website_session",
        ] {
            assert_eq!(
                report
                    .blockers
                    .iter()
                    .find(|item| item.kind == kind)
                    .unwrap()
                    .count,
                1
            );
        }
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("secret-ticket"));
        assert!(!json.contains("secret-hash"));
        // A customer receiving an application does not own the shared product itself.
        let other = check(&db.connection, "c2").unwrap();
        assert!(!other.blockers.iter().any(|item| item.kind == "product"));
    }

    #[test]
    fn delete_rechecks_and_fails_closed_on_missing_tables() {
        let mut db = Database::new();
        assert!(check(&db.connection, "c1").unwrap().can_delete);
        // Simulate a historical orphan written by a connection without FK enforcement.
        db.connection
            .pragma_update(None, "foreign_keys", "OFF")
            .unwrap();
        db.connection.execute("INSERT INTO transactions(transaction_id,order_id,customer_id,amount_cents,method,occurred_at) VALUES('orphan','missing-order','c1',100,'cash',1)", []).unwrap();
        assert_eq!(
            remove(&mut db.connection, "c1").unwrap_err().0,
            StatusCode::CONFLICT
        );
        db.connection
            .execute_batch("DELETE FROM transactions; DROP TABLE website_sessions;")
            .unwrap();
        assert_eq!(
            remove(&mut db.connection, "c1").unwrap_err().0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            db.connection
                .query_row(
                    "SELECT COUNT(*) FROM customers WHERE customer_id='c1'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn historical_destroyed_licenses_block_deletion_until_manually_removed() {
        let mut db = Database::new();
        db.connection.execute_batch(
            "INSERT INTO legacy_licenses(license_id,license,subject,modules_json,issued_at,expires_at,module_expires_at_json,platform_status,module_statuses_json,order_id) VALUES('legacy-1','token','c1','[]',1,2,'{}','destroyed','{}','missing-order');"
        ).unwrap();
        let report = check(&db.connection, "c1").unwrap();
        assert_eq!(report.blockers.len(), 1);
        assert_eq!(report.blockers[0].kind, "legacy_license");
        assert_eq!(report.blockers[0].record_ids, vec!["legacy-1"]);
        assert_eq!(
            remove(&mut db.connection, "c1").unwrap_err().0,
            StatusCode::CONFLICT
        );
        db.connection
            .execute(
                "DELETE FROM legacy_licenses WHERE license_id='legacy-1'",
                [],
            )
            .unwrap();
        assert!(remove(&mut db.connection, "c1").unwrap().can_delete);
    }

    #[tokio::test]
    async fn deletion_endpoints_require_platform_admin() {
        let db = Database::new();
        let database_path = db.root.join("test.sqlite");
        ops::initialize(&database_path).unwrap();
        db.connection
            .execute(
                "UPDATE users SET role='platform_operator' WHERE username='admin'",
                [],
            )
            .unwrap();
        let (token, _) = staff_sessions::login(
            &db.connection,
            LoginRequest {
                username: "admin".into(),
                password: "test-password".into(),
            },
            Utc::now().timestamp_millis(),
        )
        .unwrap();
        let state = AppState {
            signing_key: Arc::new(EncodingKey::from_secret(b"test")),
            verification_key: Arc::new(DecodingKey::from_secret(b"test")),
            database_path,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            ops: Arc::new(crate::ops::OpsState::new(
                false,
                PathBuf::from("unused-agent.sock"),
            )),
        };
        assert_eq!(
            get_eligibility(State(state.clone()), HeaderMap::new(), Path("c1".into()))
                .await
                .unwrap_err()
                .0,
            StatusCode::UNAUTHORIZED
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_str(&format!("license_center_session={token}")).unwrap(),
        );
        assert_eq!(
            get_eligibility(State(state.clone()), headers.clone(), Path("c1".into()))
                .await
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            delete_customer(State(state), headers, Path("c1".into()))
                .await
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
    }
}
