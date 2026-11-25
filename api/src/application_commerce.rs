//! Application commerce. Checkout is explicitly simulated, sharing the AI market's UX.
//! Money is integer CNY cents. Technical releases and commercial revisions remain separate.
use super::*;
use serde_json::{Value, json};
type Failure = (StatusCode, Json<Envelope<()>>);
type ResultJson<T> = Result<Json<T>, Failure>;
fn fail(message: &str) -> Failure {
    error(StatusCode::BAD_REQUEST, message)
}
fn internal(e: impl std::fmt::Display) -> Failure {
    eprintln!("application commerce: {e}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "应用交易操作失败")
}
fn db(state: &AppState) -> Result<Connection, Failure> {
    Connection::open(&state.database_path).map_err(internal)
}
fn id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}
pub(super) fn initialize(c: &Connection) -> Result<(), String> {
    c.execute_batch(include_str!("application_commerce.sql"))
        .map_err(|e| e.to_string())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CommercialConfig {
    pub pricing_mode: String,
    pub price_cents: i64,
    pub currency: String,
    pub license_model: String,
    pub publisher_rate_basis_points: i64,
    pub description: String,
}
impl CommercialConfig {
    pub(super) fn validate(&self) -> Result<(), Failure> {
        if self.currency != "CNY"
            || self.license_model != "one_time"
            || self.description.trim().is_empty()
            || self.description.len() > 10000
            || !(0..=10000).contains(&self.publisher_rate_basis_points)
            || self.price_cents > 100_000_000
            || !match self.pricing_mode.as_str() {
                "free" => self.price_cents == 0 && self.publisher_rate_basis_points == 0,
                "paid" => self.price_cents > 0,
                _ => false,
            }
        {
            return Err(fail(
                "请填写有效商业资料：人民币免费或一次买断、有效价格、发布者分成 0–100%，免费商品分成为 0，商品说明必填",
            ));
        }
        Ok(())
    }
}
pub(super) fn event(
    c: &Connection,
    subject: &str,
    actor: &str,
    kind: &str,
    detail: Value,
) -> Result<(), Failure> {
    c.execute(
        "INSERT INTO application_commerce_events VALUES(?,?,?,?,?,?)",
        params![
            id("ace"),
            subject,
            actor,
            kind,
            detail.to_string(),
            Utc::now().timestamp()
        ],
    )
    .map_err(internal)?;
    Ok(())
}
pub(super) fn receive_config(
    c: &Connection,
    product: &str,
    release: &str,
    actor: &str,
    value: &Value,
) -> Result<(), Failure> {
    if value.is_null() {
        return Ok(());
    }
    let config: CommercialConfig =
        serde_json::from_value(value.clone()).map_err(|_| fail("商业资料格式无效"))?;
    config.validate()?;
    c.execute("INSERT INTO application_commercial_versions(id,product_id,release_id,config_json,submitted_by,created_at) VALUES(?,?,?,?,?,?)",params![id("acv"),product,release,serde_json::to_string(&config).map_err(internal)?,actor,Utc::now().timestamp()]).map_err(internal)?;
    Ok(())
}
pub(super) fn review_release(
    c: &Connection,
    release: &str,
    approved: bool,
    actor: &str,
    reason: &str,
) -> Result<(), Failure> {
    // The release review covers only its original submitted commercial snapshot.
    // Later independent price changes always require their own review.
    let original: Option<(String,String)> = c.query_row("SELECT id,status FROM application_commercial_versions WHERE release_id=? ORDER BY rowid ASC LIMIT 1",[release],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(internal)?;
    if approved
        && original
            .as_ref()
            .is_none_or(|(_, status)| status == "rejected")
    {
        return Err(fail("缺少可通过的原始商业资料，请先补齐或重新提交申请"));
    }
    if let Some((choice, status)) = original {
        if status == "pending_review" {
            c.execute("UPDATE application_commercial_versions SET status=?,reviewed_by=?,reason=?,reviewed_at=? WHERE id=? AND status='pending_review'",params![if approved{"approved"}else{"rejected"},actor,reason,Utc::now().timestamp(),choice]).map_err(internal)?;
        }
    }
    Ok(())
}
pub(super) fn select_listing(
    c: &Connection,
    product: &str,
    release: &str,
    commercial: Option<&str>,
) -> Result<String, Failure> {
    let choice: Option<String> = if let Some(choice) = commercial {
        c.query_row("SELECT id FROM application_commercial_versions WHERE id=? AND product_id=? AND release_id=? AND status='approved'",params![choice,product,release],|r|r.get(0)).optional().map_err(internal)?
    } else {
        c.query_row("SELECT id FROM application_commercial_versions WHERE product_id=? AND release_id=? AND status='approved' ORDER BY created_at DESC,rowid DESC LIMIT 1",params![product,release],|r|r.get(0)).optional().map_err(internal)?
    };
    let choice = choice.ok_or_else(|| fail("上架版本缺少已审核的商业配置"))?;
    c.execute("INSERT INTO application_sales_listing VALUES(?,?) ON CONFLICT(product_id) DO UPDATE SET commercial_id=excluded.commercial_id",params![product,choice]).map_err(internal)?;
    Ok(choice)
}
fn read_commercial(c: &Connection, choice: &str) -> Result<Value, Failure> {
    c.query_row("SELECT id,product_id,release_id,config_json,status,reason,submitted_by,reviewed_by,created_at FROM application_commercial_versions WHERE id=?",[choice],|r|{let raw:String=r.get(3)?; Ok(json!({"id":r.get::<_,String>(0)?,"productId":r.get::<_,String>(1)?,"releaseId":r.get::<_,String>(2)?,"config":serde_json::from_str::<Value>(&raw).unwrap_or_default(),"status":r.get::<_,String>(4)?,"reason":r.get::<_,String>(5)?,"submittedBy":r.get::<_,String>(6)?,"reviewedBy":r.get::<_,Option<String>>(7)?,"createdAt":r.get::<_,i64>(8)?}))}).optional().map_err(internal)?.ok_or_else(||fail("商业配置不存在"))
}
pub(super) fn catalogue(
    c: &Connection,
    products: &mut Vec<Value>,
    subject: Option<&str>,
    public: bool,
) -> Result<(), Failure> {
    for p in products.iter_mut() {
        let product = p["productId"].as_str().unwrap_or("").to_string();
        let choice: Option<String> = c
            .query_row(
                "SELECT commercial_id FROM application_sales_listing WHERE product_id=?",
                [&product],
                |r| r.get(0),
            )
            .optional()
            .map_err(internal)?;
        let commercial = choice.map(|v| read_commercial(c, &v)).transpose()?;
        if let Some(commercial) =
            commercial.filter(|v| v["status"] == "approved" && v["releaseId"] == p["releaseId"])
        {
            p["commercialId"] = commercial["id"].clone();
            for key in [
                "pricingMode",
                "priceCents",
                "currency",
                "licenseModel",
                "description",
            ] {
                p[key] = commercial["config"][key].clone();
            }
            p["commercialReady"] = json!(true);
            if !public {
                p["publisherRateBasisPoints"] =
                    commercial["config"]["publisherRateBasisPoints"].clone();
            }
        } else {
            p["commercialReady"] = json!(false);
        }
        if let Some(subject) = subject {
            p["owned"] = json!(entitled(c, subject, &product)?);
            p["pendingOrderId"]=c.query_row("SELECT order_id FROM application_order_details WHERE subject=? AND product_id=? AND status='pending_payment' ORDER BY created_at DESC LIMIT 1",params![subject,product],|r|r.get::<_,String>(0)).optional().map_err(internal)?.map(Value::String).unwrap_or(Value::Null);
        }
        if public {
            if let Some(o) = p.as_object_mut() {
                for key in ["dependencies", "entitlementCount", "pendingCount"] {
                    o.remove(key);
                }
            }
        }
    }
    if public {
        products.retain(|p| {
            (p["commercialReady"] == true && p["listingStatus"] == "listed")
                || p["owned"] == true
                || subject.is_some_and(|s| p["publisherSubject"].as_str() == Some(s))
        });
    }
    Ok(())
}
pub(super) fn entitled(c: &Connection, subject: &str, product: &str) -> Result<bool, Failure> {
    c.query_row("SELECT EXISTS(SELECT 1 FROM application_entitlements WHERE subject=? AND product_id=? AND status='active' AND (expires_at IS NULL OR expires_at>?))",params![subject,product,Utc::now().timestamp()],|r|r.get(0)).map_err(internal)
}
fn refresh_entitlement(c: &Connection, subject: &str, product: &str) -> Result<(), Failure> {
    let active:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM application_entitlement_sources WHERE subject=? AND product_id=? AND status='active' AND (expires_at IS NULL OR expires_at>?))",params![subject,product,Utc::now().timestamp()],|r|r.get(0)).map_err(internal)?;
    let expires:Option<i64>=c.query_row("SELECT CASE WHEN SUM(CASE WHEN expires_at IS NULL THEN 1 ELSE 0 END)>0 THEN NULL ELSE MAX(expires_at) END FROM application_entitlement_sources WHERE subject=? AND product_id=? AND status='active' AND (expires_at IS NULL OR expires_at>?)",params![subject,product,Utc::now().timestamp()],|r|r.get(0)).map_err(internal)?;
    c.execute("INSERT INTO application_entitlements(subject,product_id,status,expires_at,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(subject,product_id) DO UPDATE SET status=excluded.status,expires_at=excluded.expires_at,updated_at=excluded.updated_at",params![subject,product,if active{"active"}else{"revoked"},expires,Utc::now().timestamp(),Utc::now().timestamp()]).map_err(internal)?;
    Ok(())
}
pub(super) fn manual_grant(
    c: &Connection,
    subject: &str,
    product: &str,
    expires: Option<i64>,
) -> Result<(), Failure> {
    c.execute("INSERT INTO application_entitlement_sources VALUES(?,?,?,'gift','active',?,?) ON CONFLICT(id) DO UPDATE SET status='active',expires_at=excluded.expires_at",params![format!("gift:{subject}:{product}"),subject,product,expires,Utc::now().timestamp()]).map_err(internal)?;
    refresh_entitlement(c, subject, product)
}
pub(super) fn revoke(c: &Connection, subject: &str, product: &str) -> Result<(), Failure> {
    c.execute("UPDATE application_entitlement_sources SET status='revoked' WHERE subject=? AND product_id=?",params![subject,product]).map_err(internal)?;
    refresh_entitlement(c, subject, product)
}
fn customer_subject(headers: &HeaderMap, state: &AppState) -> Result<String, Failure> {
    let claims = bearer_token(headers)
        .and_then(|t| verify_license(t, state).ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "平台许可证无效"))?;
    let active: bool = db(state)?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=? AND status='active')",
            [&claims.subject],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if !active {
        return Err(error(StatusCode::FORBIDDEN, "客户不存在或已停用"));
    }
    Ok(claims.subject)
}
fn order(c: &Connection, order_id: &str, subject: Option<&str>) -> Result<Value, Failure> {
    let row:Option<(String,String,String,String,String,String,i64,String)>=c.query_row("SELECT a.subject,a.product_id,a.publisher_subject,a.release_id,a.snapshot_json,a.status,o.total_amount_cents,o.order_no FROM application_order_details a JOIN orders o ON o.order_id=a.order_id WHERE a.order_id=?",[order_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional().map_err(internal)?;
    let (owner, product, publisher, release, snapshot, status, amount, number) =
        row.ok_or_else(|| error(StatusCode::NOT_FOUND, "应用订单不存在"))?;
    if subject.is_some_and(|s| s != owner) {
        return Err(error(StatusCode::NOT_FOUND, "应用订单不存在"));
    }
    let mut snapshot: Value = serde_json::from_str(&snapshot).map_err(internal)?;
    if subject.is_some() {
        if let Some(o) = snapshot.as_object_mut() {
            o.remove("publisherRateBasisPoints");
        }
    }
    let reason: String = c
        .query_row(
            "SELECT refund_reason FROM application_order_details WHERE order_id=?",
            [order_id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    Ok(
        json!({"refundReason":reason,"orderId":order_id,"orderNo":number,"subject":owner,"productId":product,"publisherSubject":publisher,"releaseId":release,"snapshot":snapshot,"status":status,"amountCents":amount,"simulation":true,"owned":entitled(c,&owner,&product)?}),
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CheckoutRequest {
    idempotency_key: String,
    commercial_id: String,
}
pub(super) async fn checkout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
    Json(request): Json<CheckoutRequest>,
) -> ResultJson<Envelope<Value>> {
    let subject = customer_subject(&headers, &state)?;
    if request.idempotency_key.is_empty() || request.idempotency_key.len() > 128 {
        return Err(fail("订单幂等键无效"));
    }
    let mut c = db(&state)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let previous:Option<(String,String)>=tx.query_row("SELECT order_id,product_id FROM application_order_details WHERE subject=? AND idempotency_key=?",params![subject,request.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(internal)?;
    if let Some((previous, target)) = previous {
        if target != product {
            return Err(error(StatusCode::CONFLICT, "幂等键已经用于其他商品"));
        }
        return Ok(Json(success(
            "订单已创建",
            order(&tx, &previous, Some(&subject))?,
        )));
    }
    let pending:Option<String>=tx.query_row("SELECT order_id FROM application_order_details WHERE subject=? AND product_id=? AND status='pending_payment'",params![subject,product],|r|r.get(0)).optional().map_err(internal)?;
    if let Some(previous) = pending {
        return Ok(Json(success(
            "继续待支付订单",
            order(&tx, &previous, Some(&subject))?,
        )));
    }
    if entitled(&tx, &subject, &product)? {
        return Err(error(StatusCode::CONFLICT, "已拥有此商品，无需重复购买"));
    }
    let row:Option<(String,String,String,String,String)>=tx.query_row("SELECT p.name,p.publisher_subject,l.release_id,v.id,v.config_json FROM application_products p JOIN application_listing_versions l ON l.product_id=p.product_id JOIN application_sales_listing s ON s.product_id=p.product_id JOIN application_commercial_versions v ON v.id=s.commercial_id JOIN application_releases r ON r.submission_id=l.release_id LEFT JOIN customers pub ON pub.customer_id=p.publisher_subject WHERE p.product_id=? AND p.status='active' AND l.listing_status='listed' AND r.status='approved' AND v.status='approved' AND v.release_id=l.release_id AND (p.publisher_subject='yaya_official' OR pub.status='active')",[&product],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(internal)?;
    let (name, publisher, release, commercial, raw) =
        row.ok_or_else(|| fail("商品未上架或商业资料尚未审核"))?;
    if commercial != request.commercial_id {
        return Err(error(
            StatusCode::CONFLICT,
            "价格或商业配置已变化，请刷新确认",
        ));
    }
    if publisher == subject {
        return Err(fail("发布者使用源应用即可，无需购买自己的商品"));
    }
    let config: CommercialConfig = serde_json::from_str(&raw).map_err(internal)?;
    config.validate()?;
    let order_id = id("appord");
    let now = Utc::now().timestamp();
    let mut snapshot = serde_json::to_value(&config).map_err(internal)?;
    snapshot["name"] = json!(name);
    snapshot["upgradePolicy"] = json!("same_product_compatible_versions");
    let items = json!([{"kind":"application","referenceId":product,"title":name,"quantity":1,"unitPriceCents":config.price_cents,"entitlementDays":null}]);
    tx.execute("INSERT INTO orders(order_id,order_no,customer_id,items_json,total_amount_cents,status,created_at,notes) VALUES(?,?,?,?,?,'pending_payment',?,'应用市场模拟交易，不代表真实收款')",params![order_id,order_id.to_uppercase(),subject,items.to_string(),config.price_cents,now]).map_err(internal)?;
    tx.execute("INSERT INTO application_order_details(order_id,subject,product_id,publisher_subject,release_id,commercial_id,snapshot_json,status,idempotency_key,created_at) VALUES(?,?,?,?,?,?,?,'pending_payment',?,?)",params![order_id,subject,product,publisher,release,commercial,snapshot.to_string(),request.idempotency_key,now]).map_err(internal)?;
    if config.price_cents == 0 {
        complete(&tx, &order_id, &subject)?;
    }
    event(
        &tx,
        &subject,
        &subject,
        "checkout",
        json!({"orderId":order_id,"simulation":true}),
    )?;
    let result = order(&tx, &order_id, Some(&subject))?;
    tx.commit().map_err(internal)?;
    Ok(Json(success("应用订单已创建", result)))
}
fn complete(c: &Connection, order_id: &str, subject: &str) -> Result<(), Failure> {
    let info = order(c, order_id, Some(subject))?;
    if info["status"] == "completed" {
        return Ok(());
    }
    if info["status"] != "pending_payment" {
        return Err(error(StatusCode::CONFLICT, "订单已关闭或不可支付"));
    }
    let now = Utc::now().timestamp();
    let amount = info["amountCents"].as_i64().unwrap();
    let product = info["productId"].as_str().unwrap();
    let raw: String = c
        .query_row(
            "SELECT snapshot_json FROM application_order_details WHERE order_id=?",
            [order_id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    let config: Value = serde_json::from_str(&raw).map_err(internal)?;
    let rate = config["publisherRateBasisPoints"].as_i64().unwrap();
    if amount > 0 {
        c.execute("INSERT INTO transactions(transaction_id,order_id,customer_id,amount_cents,method,reference,notes,occurred_at) VALUES(?,?,?,?,'test',?,'应用市场模拟支付，无实际资金流转',?)",params![format!("app_pay_{order_id}"),order_id,subject,amount,format!("SIM-{order_id}"),now]).map_err(internal)?;
        let share = amount * rate / 10000;
        c.execute("INSERT INTO application_revenue_entries(id,order_id,publisher_subject,product_id,amount_cents,platform_cents,gross_cents,rate_basis_points,status,created_at) VALUES(?,?,?,?,?,?,?,?,'pending_settlement',?)",params![format!("revenue_{order_id}"),order_id,info["publisherSubject"].as_str().unwrap(),product,share,amount-share,amount,rate,now]).map_err(internal)?;
    }
    c.execute(
        "INSERT INTO application_entitlement_sources VALUES(?,?,?,?,'active',NULL,?)",
        params![
            format!("order:{order_id}"),
            subject,
            product,
            if amount == 0 {
                "free_claim"
            } else {
                "simulated_purchase"
            },
            now
        ],
    )
    .map_err(internal)?;
    refresh_entitlement(c, subject, product)?;
    c.execute(
        "UPDATE application_order_details SET status='completed' WHERE order_id=?",
        [order_id],
    )
    .map_err(internal)?;
    c.execute(
        "UPDATE orders SET status='fulfilled',paid_at=? WHERE order_id=?",
        params![now, order_id],
    )
    .map_err(internal)?;
    Ok(())
}
pub(super) async fn simulate_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> ResultJson<Envelope<Value>> {
    let subject = customer_subject(&headers, &state)?;
    let mut c = db(&state)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let info = order(&tx, &order_id, Some(&subject))?;
    if info["status"] == "pending_payment" {
        let listed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM application_listing_versions l JOIN application_products p ON p.product_id=l.product_id LEFT JOIN customers c ON c.customer_id=p.publisher_subject WHERE l.product_id=? AND l.listing_status='listed' AND p.status='active' AND (p.publisher_subject='yaya_official' OR c.status='active'))",[info["productId"].as_str().unwrap()],|r|r.get(0)).map_err(internal)?;
        if !listed {
            return Err(fail("商品已下架或发布者已停用，不能继续支付"));
        }
    }
    complete(&tx, &order_id, &subject)?;
    event(
        &tx,
        &subject,
        &subject,
        "simulated_payment",
        json!({"orderId":order_id}),
    )?;
    let result = order(&tx, &order_id, Some(&subject))?;
    tx.commit().map_err(internal)?;
    Ok(Json(success("模拟支付完成，整包权益已生效", result)))
}
fn order_list(c: &Connection, subject: Option<&str>) -> Result<Vec<Value>, Failure> {
    let mut st=c.prepare("SELECT order_id FROM application_order_details WHERE (?1 IS NULL OR subject=?1) ORDER BY created_at DESC,rowid DESC").map_err(internal)?;
    let ids = st
        .query_map([subject], |r| r.get::<_, String>(0))
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    ids.iter().map(|i| order(c, i, subject)).collect()
}
pub(super) async fn my_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ResultJson<Envelope<Vec<Value>>> {
    let subject = customer_subject(&headers, &state)?;
    Ok(Json(success(
        "应用订单已读取",
        order_list(&db(&state)?, Some(&subject))?,
    )))
}
pub(super) async fn order_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<Value>,
) -> ResultJson<Envelope<Value>> {
    let subject = customer_subject(&headers, &state)?;
    let mut c = db(&state)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let info = order(&tx, &order_id, Some(&subject))?;
    match request["action"].as_str() {
        Some("cancel") if info["status"] == "pending_payment" => {
            tx.execute(
                "UPDATE application_order_details SET status='closed' WHERE order_id=?",
                [&order_id],
            )
            .map_err(internal)?;
            tx.execute(
                "UPDATE orders SET status='cancelled' WHERE order_id=?",
                [&order_id],
            )
            .map_err(internal)?;
        }
        Some("refund") if info["status"] == "completed" => {
            let reason = request["reason"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= 2000)
                .ok_or_else(|| fail("退款原因必填，最多 2000 字节"))?;
            tx.execute("UPDATE application_order_details SET status='refund_requested',refund_reason=? WHERE order_id=?",params![reason,order_id]).map_err(internal)?;
            tx.execute("UPDATE application_revenue_entries SET status='frozen' WHERE order_id=? AND status='pending_settlement'",[&order_id]).map_err(internal)?;
        }
        _ => return Err(error(StatusCode::CONFLICT, "当前状态不能执行该操作")),
    }
    event(
        &tx,
        &subject,
        &subject,
        "order_action",
        json!({"orderId":order_id,"request":request}),
    )?;
    let result = order(&tx, &order_id, Some(&subject))?;
    tx.commit().map_err(internal)?;
    Ok(Json(success("订单已更新", result)))
}
async fn publication_actor(
    headers: &HeaderMap,
    state: &AppState,
    product: &str,
) -> Result<String, Failure> {
    if bearer_token(headers).is_some() {
        let subject = customer_subject(headers, state)?;
        let owner: Option<String> = db(state)?
            .query_row(
                "SELECT publisher_subject FROM application_products WHERE product_id=?",
                [product],
                |r| r.get(0),
            )
            .optional()
            .map_err(internal)?;
        if owner.as_deref() != Some(subject.as_str()) {
            return Err(error(
                StatusCode::FORBIDDEN,
                "只能查看和提交自己商品的商业资料",
            ));
        }
        Ok(subject)
    } else {
        Ok(require_admin(headers, state).await?.user_id)
    }
}
pub(super) async fn list_commercial(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
) -> ResultJson<Envelope<Vec<Value>>> {
    publication_actor(&headers, &state, &product).await?;
    let c = db(&state)?;
    let mut st=c.prepare("SELECT id FROM application_commercial_versions WHERE product_id=? ORDER BY created_at DESC,rowid DESC").map_err(internal)?;
    let ids = st
        .query_map([product], |r| r.get::<_, String>(0))
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(Json(success(
        "商业配置已读取",
        ids.iter()
            .map(|i| read_commercial(&c, i))
            .collect::<Result<Vec<_>, _>>()?,
    )))
}
pub(super) async fn submit_commercial(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
    Json(request): Json<Value>,
) -> ResultJson<Envelope<()>> {
    let actor = publication_actor(&headers, &state, &product).await?;
    let mut c = db(&state)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let release = request["releaseId"]
        .as_str()
        .ok_or_else(|| fail("请选择发布版本"))?;
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM application_releases WHERE submission_id=? AND product_id=?)",params![release,product],|r|r.get(0)).map_err(internal)?;
    if !exists {
        return Err(fail("版本不属于该商品"));
    }
    receive_config(&tx, &product, release, &actor, &request["config"])?;
    if request["config"].is_null() {
        return Err(fail("商业资料必填"));
    }
    event(&tx, &product, &actor, "commercial_submit", request)?;
    tx.commit().map_err(internal)?;
    Ok(Json(success("商业配置已提交，等待审核", ())))
}
pub(super) async fn review_commercial(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(choice): Path<String>,
    Json(request): Json<Value>,
) -> ResultJson<Envelope<()>> {
    let actor = require_admin(&headers, &state).await?;
    let mut c = db(&state)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let approved = request["approved"]
        .as_bool()
        .ok_or_else(|| fail("审核结果必填"))?;
    let reason = request["reason"].as_str().unwrap_or("");
    if !approved && reason.trim().is_empty() {
        return Err(fail("驳回原因必填"));
    }
    let count=tx.execute("UPDATE application_commercial_versions SET status=?,reviewed_by=?,reason=?,reviewed_at=? WHERE id=? AND status='pending_review'",params![if approved{"approved"}else{"rejected"},actor.user_id,reason,Utc::now().timestamp(),choice]).map_err(internal)?;
    if count != 1 {
        return Err(error(StatusCode::CONFLICT, "商业配置不存在或已审核"));
    }
    event(&tx, &choice, &actor.user_id, "commercial_review", request)?;
    tx.commit().map_err(internal)?;
    Ok(Json(success("商业审核完成，请重新选择版本上架以生效", ())))
}
fn revenues(c: &Connection, subject: Option<&str>) -> Result<Vec<Value>, Failure> {
    let mut st=c.prepare("SELECT id,order_id,publisher_subject,product_id,amount_cents,platform_cents,gross_cents,rate_basis_points,status,settlement_id FROM application_revenue_entries WHERE (?1 IS NULL OR publisher_subject=?1) ORDER BY created_at DESC,rowid DESC").map_err(internal)?;
    st.query_map([subject],|r|Ok(json!({"id":r.get::<_,String>(0)?,"orderId":r.get::<_,String>(1)?,"publisherSubject":r.get::<_,String>(2)?,"productId":r.get::<_,String>(3)?,"amountCents":r.get::<_,i64>(4)?,"platformCents":r.get::<_,i64>(5)?,"grossCents":r.get::<_,i64>(6)?,"rateBasisPoints":r.get::<_,i64>(7)?,"status":r.get::<_,String>(8)?,"settlementId":r.get::<_,Option<String>>(9)?,"simulation":true}))).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)
}
pub(super) async fn publisher_revenues(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ResultJson<Envelope<Vec<Value>>> {
    let subject = customer_subject(&headers, &state)?;
    Ok(Json(success(
        "模拟销售收益已读取",
        revenues(&db(&state)?, Some(&subject))?,
    )))
}
pub(super) async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ResultJson<Envelope<Value>> {
    require_admin(&headers, &state).await?;
    let c = db(&state)?;
    let mut st=c.prepare("SELECT id,publisher_subject,amount_cents,status,created_at FROM application_settlements ORDER BY created_at DESC").map_err(internal)?;
    let settlements=st.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"publisherSubject":r.get::<_,String>(1)?,"amountCents":r.get::<_,i64>(2)?,"status":r.get::<_,String>(3)?,"createdAt":r.get::<_,i64>(4)?,"simulation":true}))).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)?;
    Ok(Json(success(
        "应用模拟交易台账",
        json!({"orders":order_list(&c,None)?,"revenues":revenues(&c,None)?,"settlements":settlements,"simulation":true}),
    )))
}
pub(super) async fn refund(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<Value>,
) -> ResultJson<Envelope<()>> {
    let actor = require_admin(&headers, &state).await?;
    let mut c = db(&state)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let info = order(&tx, &order_id, None)?;
    if info["status"] == "refunded" {
        return Ok(Json(success("已完成模拟退款", ())));
    }
    if info["status"] != "refund_requested" {
        return Err(error(StatusCode::CONFLICT, "订单不在退款申请状态"));
    }
    let approve = request["approved"]
        .as_bool()
        .ok_or_else(|| fail("退款审核结果必填"))?;
    if approve {
        let now = Utc::now().timestamp();
        let amount = info["amountCents"].as_i64().unwrap();
        let subject = info["subject"].as_str().unwrap();
        let product = info["productId"].as_str().unwrap();
        tx.execute("UPDATE application_revenue_entries SET status='pending_settlement' WHERE order_id=? AND status='frozen'",[&order_id]).map_err(internal)?;
        if amount > 0 {
            tx.execute("INSERT INTO transactions(transaction_id,order_id,customer_id,amount_cents,method,reference,notes,occurred_at) VALUES(?,?,?,?,'test',?,'应用市场模拟退款，无实际资金流转',?)",params![format!("app_refund_{order_id}"),order_id,subject,-amount,format!("SIM-REFUND-{order_id}"),now]).map_err(internal)?;
            tx.execute("INSERT INTO application_revenue_entries(id,order_id,publisher_subject,product_id,amount_cents,platform_cents,gross_cents,rate_basis_points,status,created_at,reverses_id) SELECT ?,order_id,publisher_subject,product_id,-amount_cents,-platform_cents,-gross_cents,rate_basis_points,'pending_settlement',?,id FROM application_revenue_entries WHERE id=?",params![format!("reversal_{order_id}"),now,format!("revenue_{order_id}")]).map_err(internal)?;
        }
        tx.execute(
            "UPDATE application_entitlement_sources SET status='revoked' WHERE id=?",
            [format!("order:{order_id}")],
        )
        .map_err(internal)?;
        refresh_entitlement(&tx, subject, product)?;
        tx.execute(
            "UPDATE application_order_details SET status='refunded' WHERE order_id=?",
            [&order_id],
        )
        .map_err(internal)?;
        tx.execute(
            "UPDATE orders SET status='cancelled' WHERE order_id=?",
            [&order_id],
        )
        .map_err(internal)?;
    } else {
        if request["reason"]
            .as_str()
            .is_none_or(|r| r.trim().is_empty())
        {
            return Err(fail("拒绝退款请填写原因"));
        }
        tx.execute("UPDATE application_revenue_entries SET status='pending_settlement' WHERE order_id=? AND status='frozen'",[&order_id]).map_err(internal)?;
        tx.execute(
            "UPDATE application_order_details SET status='completed' WHERE order_id=?",
            [&order_id],
        )
        .map_err(internal)?;
    }
    event(
        &tx,
        info["subject"].as_str().unwrap(),
        &actor.user_id,
        "simulated_refund",
        json!({"orderId":order_id,"review":request}),
    )?;
    tx.commit().map_err(internal)?;
    Ok(Json(success(
        "模拟退款审核已完成，不删除客户应用或业务数据",
        (),
    )))
}
pub(super) async fn settle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> ResultJson<Envelope<Value>> {
    let actor = require_admin(&headers, &state).await?;
    let subject = request["publisherSubject"]
        .as_str()
        .ok_or_else(|| fail("发布主体必填"))?;
    let mut c = db(&state)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let amount:i64=tx.query_row("SELECT COALESCE(SUM(amount_cents),0) FROM application_revenue_entries WHERE publisher_subject=? AND status='pending_settlement'",[subject],|r|r.get(0)).map_err(internal)?;
    if amount <= 0 {
        return Err(fail("暂无正向可结算模拟收益，退款抵扣将保留至后续结算"));
    }
    let batch = id("appset");
    let now = Utc::now().timestamp();
    tx.execute(
        "INSERT INTO application_settlements VALUES(?,?,?,'simulated',?,?)",
        params![batch, subject, amount, actor.user_id, now],
    )
    .map_err(internal)?;
    tx.execute("UPDATE application_revenue_entries SET status='settled',settlement_id=? WHERE publisher_subject=? AND status='pending_settlement'",params![batch,subject]).map_err(internal)?;
    event(
        &tx,
        subject,
        &actor.user_id,
        "simulated_settlement",
        json!({"settlementId":batch,"amountCents":amount}),
    )?;
    tx.commit().map_err(internal)?;
    Ok(Json(success(
        "模拟结算已登记，无实际打款",
        json!({"id":batch,"amountCents":amount,"simulation":true}),
    )))
}
