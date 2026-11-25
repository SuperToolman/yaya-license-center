//! Customer authorization is stable. Signed licenses are immutable versions produced by paid orders.
use super::*;
#[cfg(test)]
mod tests;
type Failure = (StatusCode, Json<Envelope<()>>);
const SQL: &str = r#"
CREATE TABLE customer_authorizations (
 authorization_id TEXT PRIMARY KEY,
 customer_id TEXT NOT NULL UNIQUE REFERENCES customers(customer_id),
 latest_license_id TEXT REFERENCES licenses(license_id),
 active_license_id TEXT REFERENCES licenses(license_id),
 revision INTEGER NOT NULL DEFAULT 0,
 updated_at INTEGER NOT NULL
);
CREATE TABLE authorization_versions (
 license_id TEXT PRIMARY KEY REFERENCES licenses(license_id),
 authorization_id TEXT NOT NULL REFERENCES customer_authorizations(authorization_id),
 revision INTEGER NOT NULL,
 order_id TEXT NOT NULL UNIQUE REFERENCES orders(order_id),
 state TEXT NOT NULL CHECK(state IN ('pending','active','superseded','revoked')),
 created_at INTEGER NOT NULL,
 UNIQUE(authorization_id,revision)
);
CREATE TABLE order_fulfillments (
 order_id TEXT PRIMARY KEY REFERENCES orders(order_id),
 kind TEXT NOT NULL CHECK(kind IN ('authorization','service')),
 license_id TEXT REFERENCES licenses(license_id),
 actor_user_id TEXT NOT NULL,
 created_at INTEGER NOT NULL
);
"#;
pub(super) fn initialize(db: &mut Connection) -> Result<(), String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS licensing_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL)").map_err(|e|e.to_string())?;
    let checksum = format!("{:x}", Sha256::digest(SQL.replace("\r\n", "\n").as_bytes()));
    let saved: Option<String> = tx
        .query_row(
            "SELECT checksum FROM licensing_migrations WHERE version=1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(saved) = saved {
        if saved != checksum {
            return Err("Licensing migration checksum mismatch (version 1)".into());
        }
    } else {
        tx.execute_batch(SQL).map_err(|e| e.to_string())?;
        let rows = {
            let mut q=tx.prepare("SELECT license_id,subject,order_id,platform_status,issued_at FROM licenses WHERE linkage_status='linked' AND order_id IS NOT NULL ORDER BY issued_at,license_id").map_err(|e|e.to_string())?;
            q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?
        };
        for (id, customer, order, status, time) in rows {
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=?)",
                    [&customer],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !exists {
                continue;
            }
            let auth = ensure(&tx, &customer, time).map_err(|e| e.to_string())?;
            let rev:i64=tx.query_row("UPDATE customer_authorizations SET revision=revision+1,updated_at=? WHERE authorization_id=? RETURNING revision",params![time,auth],|r|r.get(0)).map_err(|e|e.to_string())?;
            let state = if status == "destroyed" {
                "revoked"
            } else if status == "running" {
                "active"
            } else {
                "pending"
            };
            tx.execute(
                "INSERT INTO authorization_versions VALUES(?,?,?,?,?,?)",
                params![id, auth, rev, order, state, time],
            )
            .map_err(|e| e.to_string())?;
            {
                tx.execute("UPDATE customer_authorizations SET latest_license_id=? WHERE authorization_id=?",params![id,auth]).map_err(|e|e.to_string())?;
            }
            if status == "running" {
                tx.execute("UPDATE customer_authorizations SET active_license_id=? WHERE authorization_id=?",params![id,auth]).map_err(|e|e.to_string())?;
            }
            tx.execute("INSERT OR IGNORE INTO order_fulfillments VALUES(?,'authorization',?,'migration',?)",params![order,id,time]).map_err(|e|e.to_string())?;
        }
        tx.execute("UPDATE customer_authorizations SET active_license_id=NULL WHERE latest_license_id IN (SELECT license_id FROM licenses WHERE platform_status='destroyed')",[]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE authorization_versions SET state='superseded' WHERE state='active' AND license_id NOT IN (SELECT active_license_id FROM customer_authorizations WHERE active_license_id IS NOT NULL)",[]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO licensing_migrations VALUES(1,?)", [checksum])
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
fn ensure(db: &Connection, customer: &str, now: i64) -> rusqlite::Result<String> {
    db.execute("INSERT INTO customer_authorizations(authorization_id,customer_id,updated_at) VALUES(?,?,?) ON CONFLICT(customer_id) DO NOTHING",params![format!("auth_{}",Uuid::new_v4().simple()),customer,now])?;
    db.query_row(
        "SELECT authorization_id FROM customer_authorizations WHERE customer_id=?",
        [customer],
        |r| r.get(0),
    )
}
pub(super) fn latest(db: &Connection, customer: &str) -> rusqlite::Result<Option<LicenseRecord>> {
    db.query_row("SELECT l.license_id,l.license,l.subject,l.customer_name_snapshot,l.modules_json,l.issued_at,l.expires_at,l.module_expires_at_json,l.ai_employees_json,l.platform_status,l.module_statuses_json,l.order_id,l.linkage_status FROM customer_authorizations a JOIN licenses l ON l.license_id=a.latest_license_id WHERE a.customer_id=? AND l.platform_status<>'destroyed'",[customer],license_record_from_row).optional()
}
pub(super) fn register(
    db: &Connection,
    license: &LicenseRecord,
    actor: &str,
) -> rusqlite::Result<()> {
    let auth = ensure(db, &license.subject, license.issued_at)?;
    let rev:i64=db.query_row("UPDATE customer_authorizations SET revision=revision+1,latest_license_id=?,updated_at=? WHERE authorization_id=? RETURNING revision",params![license.license_id,license.issued_at,auth],|r|r.get(0))?;
    db.execute(
        "INSERT INTO authorization_versions VALUES(?,?,?,?, 'pending',?)",
        params![
            license.license_id,
            auth,
            rev,
            license.order_id,
            license.issued_at
        ],
    )?;
    db.execute(
        "INSERT INTO order_fulfillments VALUES(?,'authorization',?,?,?)",
        params![
            license.order_id,
            license.license_id,
            actor,
            license.issued_at
        ],
    )?;
    Ok(())
}
pub(super) fn activate(db: &Connection, license: &LicenseRecord) -> rusqlite::Result<bool> {
    let changed=db.execute("UPDATE customer_authorizations SET active_license_id=?,updated_at=? WHERE customer_id=? AND latest_license_id=?",params![license.license_id,Utc::now().timestamp(),license.subject,license.license_id])?;
    if changed != 1 {
        return Ok(false);
    }
    db.execute("UPDATE authorization_versions SET state='superseded' WHERE authorization_id=(SELECT authorization_id FROM customer_authorizations WHERE customer_id=?) AND license_id<>? AND state IN ('active','pending')",params![license.subject,license.license_id])?;
    db.execute(
        "UPDATE authorization_versions SET state='active' WHERE license_id=?",
        [&license.license_id],
    )?;
    Ok(true)
}
pub(super) fn extend(expiry: i64, now: i64, days: u32) -> Result<i64, Failure> {
    if !(1..=36500).contains(&days) {
        return Err(error(StatusCode::BAD_REQUEST, "授权天数需为 1–36500 天"));
    }
    expiry
        .max(now)
        .checked_add(i64::from(days) * 86400)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "授权期限超出范围"))
}
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/authorizations", get(list))
        .route("/api/customers/{id}/authorization", get(customer))
        .route("/api/orders/{id}/fulfill-service", post(service))
}
pub(super) fn view(db: &Connection, customer: &str) -> Result<serde_json::Value, String> {
    let row:Option<(String,Option<String>,Option<String>,i64)>=db.query_row("SELECT authorization_id,latest_license_id,active_license_id,revision FROM customer_authorizations WHERE customer_id=?",[customer],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
    let Some((id, latest, active, revision)) = row else {
        return Ok(serde_json::Value::Null);
    };
    let versions = {
        let mut q=db.prepare("SELECT l.license_id,l.license,l.subject,l.customer_name_snapshot,l.modules_json,l.issued_at,l.expires_at,l.module_expires_at_json,l.ai_employees_json,l.platform_status,l.module_statuses_json,l.order_id,l.linkage_status,v.state,v.revision FROM authorization_versions v JOIN licenses l ON l.license_id=v.license_id WHERE v.authorization_id=? ORDER BY v.revision DESC").map_err(|e|e.to_string())?;
        q.query_map([&id], |r| {
            Ok((
                license_view(&license_record_from_row(r)?),
                r.get::<_, String>(13)?,
                r.get::<_, i64>(14)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?
    };
    let versions:Vec<_>=versions.into_iter().map(|(license,state,rev)|serde_json::json!({"license":license,"state":state,"revision":rev})).collect();
    let latest_view = versions
        .iter()
        .find(|v| v["license"]["licenseId"].as_str() == latest.as_deref())
        .map(|v| v["license"].clone());
    Ok(
        serde_json::json!({"authorizationId":id,"customerId":customer,"latestLicenseId":latest,"activeLicenseId":active,"revision":revision,"latest":latest_view,"versions":versions}),
    )
}
async fn customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<serde_json::Value>>, Failure> {
    require_admin(&headers, &state).await?;
    let db = ops::db::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "授权读取失败"))?;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=?)",
            [&id],
            |r| r.get(0),
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户读取失败"))?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "客户不存在"));
    }
    Ok(Json(success(
        "客户授权",
        view(&db, &id).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "授权读取失败"))?,
    )))
}
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<serde_json::Value>>>, Failure> {
    require_admin(&headers, &state).await?;
    let db = ops::db::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "授权读取失败"))?;
    let ids = {
        let mut q = db
            .prepare("SELECT customer_id FROM customer_authorizations ORDER BY updated_at DESC")
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "授权读取失败"))?;
        q.query_map([], |r| r.get::<_, String>(0))
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "授权读取失败"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "授权读取失败"))?
    };
    let items = ids
        .into_iter()
        .map(|id| view(&db, &id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "授权读取失败"))?;
    Ok(Json(success("客户授权", items)))
}
async fn service(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<()>>, Failure> {
    let actor = operator(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let mut db = ops::db::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| error(StatusCode::CONFLICT, "订单正在处理"))?;
    let order = get_order(&tx, &id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "订单不存在"))?;
    if order.status == "fulfilled" {
        return Ok(Json(success("服务订单已完成", ())));
    }
    if order.status != "paid" || !order.items.iter().all(|i| i.kind == "service") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "仅已付款的纯服务订单可确认服务完成",
        ));
    }
    tx.execute(
        "INSERT INTO order_fulfillments VALUES(?,'service',NULL,?,?)",
        params![id, actor.user_id, Utc::now().timestamp()],
    )
    .map_err(|_| error(StatusCode::CONFLICT, "订单已履约"))?;
    tx.execute(
        "UPDATE orders SET status='fulfilled' WHERE order_id=?",
        [id],
    )
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "履约保存失败"))?;
    tx.commit()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "履约保存失败"))?;
    Ok(Json(success("服务订单已完成", ())))
}

pub(super) fn validate_items(items:&[OrderLineItem])->Result<(),Failure> {
    let mut keys=HashSet::new();
    for item in items {
        if item.kind=="service" {continue;}
        if !["platform","module","ai_employee"].contains(&item.kind.as_str())||item.quantity!=1||!item.entitlement_days.is_some_and(|d|(1..=36500).contains(&d)) {
            return Err(error(StatusCode::BAD_REQUEST,"授权商品数量须为 1，授权天数须为 1–36500；多次购买通过续费订单顺延"));
        }
        let key=if item.kind=="platform" {"platform"}else{item.reference_id.as_deref().unwrap_or("")};
        if key.is_empty()||!keys.insert(format!("{}:{key}",item.kind)){return Err(error(StatusCode::BAD_REQUEST,"授权商品标识缺失或重复"));}
    }
    Ok(())
}

pub(super) async fn operator(headers:&HeaderMap,state:&AppState)->Result<AuthenticatedUser,Failure> {
 let user=require_authenticated(headers,state).await?;
 if !["platform_admin","platform_operator"].contains(&user.role.as_str()){return Err(error(StatusCode::FORBIDDEN,"仅运营管理员可交付授权，财务账号仅处理回款"));}
 Ok(user)
}
