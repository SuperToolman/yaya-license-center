//! Application products are immutable V3 releases; approval and listing are separate operations.
use super::*;
use serde_json::{Value, json};
pub(super) const OFFICIAL_PUBLISHER: &str = "yaya_official";

type Failure = (StatusCode, Json<Envelope<()>>);
type ResultJson<T> = Result<Json<Envelope<T>>, Failure>;
fn internal(e: impl std::fmt::Display) -> Failure {
    eprintln!("application market: {e}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "应用市场操作失败")
}
fn db(state: &AppState) -> Result<Connection, Failure> {
    Connection::open(&state.database_path).map_err(internal)
}
fn submission(connection: &Connection, id: &str) -> Result<Value, Failure> {
    let mut value: Value = connection.query_row("SELECT submission_id,app_id,app_name,version,status,submitted_by,applicant_subject,submitted_at,snapshot_json,product_id,review_reason FROM application_releases WHERE submission_id=?",[id],|r|{
        let snapshot:String=r.get(8)?;Ok(json!({"submissionId":r.get::<_,String>(0)?,"appId":r.get::<_,String>(1)?,"appName":r.get::<_,String>(2)?,"version":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"submittedBy":r.get::<_,String>(5)?,"applicantSubject":r.get::<_,String>(6)?,"submittedAt":r.get::<_,i64>(7)?,"snapshot":serde_json::from_str::<Value>(&snapshot).unwrap_or_default(),"productId":r.get::<_,Option<String>>(9)?,"reviewReason":r.get::<_,String>(10)?}))
    }).optional().map_err(internal)?.ok_or_else(||error(StatusCode::NOT_FOUND,"申请不存在"))?;
    if value["snapshot"]["submissionType"].is_null() {
        let earlier: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM application_releases old JOIN application_releases current ON old.product_id=current.product_id WHERE current.submission_id=? AND old.rowid<current.rowid)", [id], |r|r.get(0)).map_err(internal)?;
        value["snapshot"]["submissionType"] = json!(if earlier {"version_upgrade"} else {"new_listing"});
    }
    let listed: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM application_listing_versions WHERE release_id=? AND listing_status='listed')", [id], |r|r.get(0)).map_err(internal)?;
    value["listingStatus"] = json!(if listed {"listed"} else {"unlisted"});
    Ok(value)
}
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(super) async fn receive_application_submission(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> ResultJson<Value> {
    let claims = bearer_token(&headers)
        .and_then(|t| verify_license(t, &state).ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "平台许可证无效或已过期"))?;
    if claims.subject == OFFICIAL_PUBLISHER {
        return Err(error(StatusCode::FORBIDDEN, "官方应用仅允许运营端导入"));
    }
    let (mut metadata, mut bytes) = (None, None);
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| error(StatusCode::BAD_REQUEST, "上传无效"))?
    {
        match field.name() {
            Some("metadata") => metadata = Some(field.text().await.map_err(internal)?),
            Some("package") => bytes = Some(field.bytes().await.map_err(internal)?.to_vec()),
            _ => (),
        }
    }
    let request: ApplicationReleaseSubmission = serde_json::from_str(
        &metadata.ok_or_else(|| error(StatusCode::BAD_REQUEST, "缺少元数据"))?,
    )
    .map_err(|_| error(StatusCode::BAD_REQUEST, "元数据无效"))?;
    let bytes = bytes.ok_or_else(|| error(StatusCode::BAD_REQUEST, "缺少应用包"))?;
    accept_package(&state, &claims.subject, request, bytes)
}

fn accept_package(
    state: &AppState,
    publisher: &str,
    request: ApplicationReleaseSubmission,
    bytes: Vec<u8>,
) -> ResultJson<Value> {
    let (manifest, bundle) =
        application_protocol::read(&bytes).map_err(|m| error(StatusCode::BAD_REQUEST, &m))?;
    if !safe_id(&request.submission_id)
        || manifest["version"] != request.version
    {
        return Err(error(StatusCode::BAD_REQUEST, "提交元数据与 V3 包不一致"));
    }
    let kind = if request.product_id.is_some() { "version_upgrade" } else { "new_listing" };
    let intent = &manifest["publication"];
    if !intent.is_null() && (intent["submissionType"] != kind || intent["targetProductId"].as_str() != request.product_id.as_deref()) {
        return Err(error(StatusCode::BAD_REQUEST, "应用包的申请类型或目标商品与本次提交不一致，请重新导出或选择正确的目标商品"));
    }
    let sha = application_protocol::digest(&bytes);
    let request_digest = application_protocol::digest(json!({"appId":request.app_id,"appName":request.app_name,"version":request.version,"productId":request.product_id,"submittedBy":if request.snapshot["source"]=="offline-import" { Value::Null } else { json!(request.submitted_by) },"submittedAt":request.submitted_at,"snapshot":request.snapshot,"fingerprint":request.fingerprint}).to_string().as_bytes());
    let mut connection = db(&state)?;
    let txn = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let exists=txn.query_row("SELECT applicant_subject,snapshot_json FROM application_releases WHERE submission_id=?",[&request.submission_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional().map_err(internal)?;
    if let Some((owner, snapshot)) = exists {
        let snapshot: Value = serde_json::from_str(&snapshot).map_err(internal)?;
        if owner != publisher
            || snapshot["packageSha256"] != sha
            || snapshot["requestDigest"] != request_digest
        {
            return Err(error(
                StatusCode::CONFLICT,
                "申请已存在且内容不同，不能覆盖",
            ));
        }
        let result = submission(&txn, &request.submission_id)?;
        return Ok(Json(success("相同申请已接收", result)));
    }
    if let Some(name) = bundle["packageName"].as_str() {
        if name.trim().is_empty()
            || name.chars().count() > 80
            || name.chars().any(char::is_control)
            || name != request.app_name
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "套应用名称与提交资料不一致或格式无效",
            ));
        }
    }
    let product = if let Some(product) = request.product_id.as_deref() {
        let owner=txn.query_row("SELECT publisher_subject FROM application_products WHERE product_id=? AND status='active'",[product],|r|r.get::<_,String>(0)).optional().map_err(internal)?.ok_or_else(||error(StatusCode::NOT_FOUND,"应用商品不存在"))?;
        if owner != publisher {
            return Err(error(
                StatusCode::FORBIDDEN,
                "不能发布其他主体的商品",
            ));
        }
        let mut statement = txn
            .prepare("SELECT snapshot_json,version FROM application_releases WHERE product_id=?")
            .map_err(internal)?;
        for row in statement
            .query_map([product], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(internal)?
        {
            let (snapshot, version) = row.map_err(internal)?;
            if version == request.version {
                return Err(error(StatusCode::CONFLICT, "商品版本号已存在"));
            }
            let old: Value = serde_json::from_str(&snapshot).map_err(internal)?;
            for member in old["manifest"]["members"].as_array().into_iter().flatten() {
                if !bundle["members"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["appId"] == member["appId"])
                {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "不能移除原商品成员，请发布为新商品",
                    ));
                }
            }
        }
        product.to_owned()
    } else {
        let id = format!("prod_{}", Uuid::new_v4().simple());
        txn.execute("INSERT INTO application_products(product_id,publisher_subject,name,description,status) VALUES(?,?,?,?,'active')",params![id,publisher,request.app_name,""]).map_err(internal)?;
        txn.execute("INSERT INTO application_listing_versions(product_id,release_id,listing_status) VALUES(?,'','withdrawn')",[&id]).map_err(internal)?;
        id
    };
    let mut summary = json!({"applications":bundle["members"].as_array().unwrap().len()});
    for (name, table) in [
        ("forms", "form_definitions"),
        ("automations", "automation_flows"),
        ("navigation", "app_navigation_items"),
        ("views", "form_views"),
        ("buttons", "app_custom_buttons"),
    ] {
        summary[name] = json!(
            bundle["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["resources"][table].as_array().unwrap().len())
                .sum::<usize>()
        );
    }
    let snapshot = json!({"formatVersion":3,"manifest":manifest,"resourceSummary":summary,"packageSha256":sha,"requestDigest":request_digest,"submissionType":kind,"targetProductId":request.product_id,"source":if request.snapshot["source"]=="offline-import" {"offline-import"} else {"online"},"commercial":request.snapshot["commercial"]});
    fs::create_dir_all(application_package_root()).map_err(internal)?;
    let path = application_package_root().join(format!("{}.zip", request.submission_id));
    // Never overwrite a published package, even if an orphan file survived an interrupted transaction.
    if path.exists() {
        let old = fs::read(&path).map_err(internal)?;
        if application_protocol::digest(&old) != sha {
            return Err(error(StatusCode::CONFLICT, "包文件已存在且内容不同"));
        }
    } else {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(internal)?;
        std::io::Write::write_all(&mut f, &bytes).map_err(internal)?;
        f.sync_all().map_err(internal)?;
    }
    txn.execute("INSERT INTO application_releases(submission_id,app_id,app_name,version,status,submitted_by,applicant_subject,submitted_at,snapshot_json,product_id) VALUES(?,?,?,?,'pending_review',?,?,?,?,?)",params![request.submission_id,request.app_id,request.app_name,request.version,request.submitted_by,publisher,Utc::now().timestamp(),snapshot.to_string(),product]).map_err(internal)?;
    application_commerce::receive_config(
        &txn,
        &product,
        &request.submission_id,
        publisher,
        &request.snapshot["commercial"],
    )?;
    let result = submission(&txn, &request.submission_id)?;
    txn.commit().map_err(internal)?;
    Ok(Json(success("应用包已接收，等待审核", result)))
}
pub(super) async fn list_application_submissions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ResultJson<Vec<Value>> {
    require_admin(&headers, &state).await?;
    let connection = db(&state)?;
    let mut stmt = connection
        .prepare("SELECT submission_id FROM application_releases ORDER BY submitted_at DESC")
        .map_err(internal)?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(Json(success(
        "申请已读取",
        ids.iter()
            .map(|id| submission(&connection, id))
            .collect::<Result<_, _>>()?,
    )))
}
pub(super) async fn application_submission_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ResultJson<Value> {
    let claims = bearer_token(&headers)
        .and_then(|t| verify_license(t, &state).ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "许可证无效"))?;
    let value = submission(&db(&state)?, &id)?;
    if value["applicantSubject"] != claims.subject {
        return Err(error(StatusCode::FORBIDDEN, "无权读取此申请"));
    }
    Ok(Json(success("申请状态已读取", value)))
}
pub(super) async fn review_application_submission(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<ReviewApplicationSubmissionRequest>,
) -> ResultJson<Value> {
    let actor = require_admin(&headers, &state).await?;
    if !request.approved && request.reason.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "驳回申请必须填写原因"));
    }
    let mut raw = db(&state)?;
    let connection = raw
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    application_commerce::review_release(
        &connection,
        &id,
        request.approved,
        &actor.user_id,
        &request.reason,
    )?;
    let count=connection.execute("UPDATE application_releases SET status=?,reviewed_at=?,review_reason=? WHERE submission_id=? AND status='pending_review'",params![if request.approved{"approved"}else{"rejected"},Utc::now().timestamp(),request.reason.trim(),id]).map_err(internal)?;
    if count == 0 {
        return Err(error(StatusCode::CONFLICT, "申请不存在或已审核"));
    }
    let result = submission(&connection, &id)?;
    connection.commit().map_err(internal)?;
    Ok(Json(success("审核完成；通过版本需另行上架", result)))
}
// Keep immutable releases and commerce records for audit and existing entitlements.
pub(super) async fn delete_application_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
) -> ResultJson<()> {
    let actor = require_admin(&headers, &state).await?;
    let mut raw = db(&state)?;
    let tx = raw.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(internal)?;
    let publisher: String = tx.query_row("SELECT publisher_subject FROM application_products WHERE product_id=? AND status='active'", [&product], |r| r.get(0)).optional().map_err(internal)?.ok_or_else(||error(StatusCode::NOT_FOUND, "商品不存在或已删除"))?;
    let withdrawn: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM application_listing_versions WHERE product_id=? AND listing_status='withdrawn')", [&product], |r|r.get(0)).map_err(internal)?;
    if !withdrawn { return Err(error(StatusCode::CONFLICT, "请先下架商品再删除")); }
    tx.execute("UPDATE application_products SET status='deleted' WHERE product_id=?", [&product]).map_err(internal)?;
    application_commerce::event(&tx, &publisher, &actor.user_id, "delete_product", json!({"productId":product}))?;
    tx.commit().map_err(internal)?;
    Ok(Json(success("应用商品已删除", ())))
}

pub(super) async fn set_application_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
    Json(request): Json<Value>,
) -> ResultJson<()> {
    require_admin(&headers, &state).await?;
    let mut raw = db(&state)?;
    let connection = raw
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let active: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM application_products WHERE product_id=? AND status='active')", [&product], |r|r.get(0)).map_err(internal)?;
    if !active { return Err(error(StatusCode::NOT_FOUND, "商品不存在或已删除")); }
    if request["listed"] == true {
        let release = request["releaseId"]
            .as_str()
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "上架必须选择已审核版本"))?;
        let valid:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM application_releases WHERE product_id=? AND submission_id=? AND status='approved')",params![product,release],|r|r.get(0)).map_err(internal)?;
        if !valid {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "只能上架该商品审核通过的版本",
            ));
        }
        application_commerce::select_listing(
            &connection,
            &product,
            release,
            request["commercialId"].as_str(),
        )?;
        connection.execute("UPDATE application_listing_versions SET release_id=?,listing_status='listed',published_at=?,withdrawn_at=NULL WHERE product_id=?",params![release,Utc::now().timestamp(),product]).map_err(internal)?;
        connection.execute("UPDATE application_products SET name=COALESCE((SELECT json_extract(snapshot_json,'$.manifest.packageName') FROM application_releases WHERE submission_id=?),name) WHERE product_id=?",params![release,product]).map_err(internal)?;
    } else {
        connection.execute("UPDATE application_listing_versions SET listing_status='withdrawn',withdrawn_at=? WHERE product_id=?",params![Utc::now().timestamp(),product]).map_err(internal)?;
    }
    connection.commit().map_err(internal)?;
    Ok(Json(success("上架状态已更新", ())))
}
pub(super) async fn list_application_product_releases(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
) -> ResultJson<Vec<Value>> {
    require_admin(&headers, &state).await?;
    let connection = db(&state)?;
    let mut stmt=connection.prepare("SELECT submission_id FROM application_releases WHERE product_id=? AND status='approved' ORDER BY submitted_at DESC").map_err(internal)?;
    let ids = stmt
        .query_map([product], |r| r.get::<_, String>(0))
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(Json(success(
        "已审核版本已读取",
        ids.iter()
            .map(|id| submission(&connection, id))
            .collect::<Result<_, _>>()?,
    )))
}
fn products(connection: &Connection, public: bool) -> Result<Vec<Value>, Failure> {
    let query = format!(
        "SELECT p.product_id,p.publisher_subject,NULL,p.name,p.description,COALESCE(NULLIF(l.release_id,''),r.submission_id),l.listing_status,r.version,r.snapshot_json,(SELECT COUNT(*) FROM application_entitlements e WHERE e.product_id=p.product_id AND e.status='active' AND (e.expires_at IS NULL OR e.expires_at>strftime('%s','now'))),(SELECT COUNT(*) FROM application_releases x WHERE x.product_id=p.product_id AND x.status='pending_review') FROM application_products p LEFT JOIN application_listing_versions l ON l.product_id=p.product_id LEFT JOIN application_releases r ON r.submission_id=CASE WHEN l.release_id!='' THEN l.release_id ELSE (SELECT submission_id FROM application_releases x WHERE x.product_id=p.product_id ORDER BY submitted_at DESC LIMIT 1) END {} ORDER BY p.name",
        if public {
            "WHERE p.status='active' AND l.listing_status='listed'"
        } else {
            "WHERE p.status='active'"
        }
    );
    let mut stmt = connection.prepare(&query).map_err(internal)?;
    let rows=stmt.query_map([],|r|{
        let snapshot:Option<String>=r.get(8)?;let snapshot:Value=snapshot.and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default();let manifest=&snapshot["manifest"];
        Ok(json!({"productId":r.get::<_,String>(0)?,"publisherSubject":r.get::<_,String>(1)?,"name":manifest["packageName"].as_str().map(str::to_string).unwrap_or(r.get::<_,String>(3)?),"description":r.get::<_,String>(4)?,"releaseId":r.get::<_,Option<String>>(5)?.unwrap_or_default(),"listingStatus":r.get::<_,Option<String>>(6)?.unwrap_or_else(||"withdrawn".into()),"status":r.get::<_,Option<String>>(6)?.unwrap_or_else(||"withdrawn".into()),"version":r.get::<_,Option<String>>(7)?.unwrap_or_default(),"entitlementCount":r.get::<_,i64>(9)?,"pendingCount":r.get::<_,i64>(10)?,"applicationType":manifest["applicationType"],"members":manifest["members"].as_array().into_iter().flatten().map(|m|json!({"appId":m["appId"],"name":m["resources"]["apps"][0]["name"]})).collect::<Vec<_>>(),"dependencies":manifest["dependencies"],"resourceSummary":snapshot["resourceSummary"]}))
    }).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)?;
    Ok(rows)
}
pub(super) async fn list_application_products(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ResultJson<Vec<Value>> {
    require_admin(&headers, &state).await?;
    Ok(Json(success("应用商品已读取", {
        let c = db(&state)?;
        let mut items = products(&c, false)?;
        application_commerce::catalogue(&c, &mut items, None, false)?;
        items
    })))
}
pub(super) async fn list_public_applications(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ResultJson<Vec<Value>> {
    let claims = bearer_token(&headers)
        .and_then(|t| verify_license(t, &state).ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "许可证无效"))?;
    Ok(Json(success("应用目录已读取", {
        let c = db(&state)?;
        let mut items = products(&c, false)?;
        application_commerce::catalogue(&c, &mut items, Some(&claims.subject), true)?;
        items
    })))
}
pub(super) async fn get_public_application(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ResultJson<Value> {
    let claims = bearer_token(&headers)
        .and_then(|t| verify_license(t, &state).ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "许可证无效"))?;
    let c = db(&state)?;
    let mut items = products(&c, false)?;
    application_commerce::catalogue(&c, &mut items, Some(&claims.subject), true)?;
    let product = items
        .into_iter()
        .find(|p| p["productId"] == id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "应用未上架"))?;
    Ok(Json(success("应用详情已读取", product)))
}
pub(super) async fn list_application_entitlements(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
) -> ResultJson<Vec<Value>> {
    require_admin(&headers, &state).await?;
    let connection = db(&state)?;
    let mut stmt = connection
        .prepare(
            "SELECT subject,status,expires_at FROM application_entitlements WHERE product_id=?",
        )
        .map_err(internal)?;
    let rows=stmt.query_map([product],|r|Ok(json!({"subject":r.get::<_,String>(0)?,"status":r.get::<_,String>(1)?,"expiresAt":r.get::<_,Option<i64>>(2)?}))).map_err(internal)?.collect::<Result<_,_>>().map_err(internal)?;
    Ok(Json(success("授权已读取", rows)))
}
pub(super) async fn grant_application_entitlement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
    Json(request): Json<GrantApplicationEntitlementRequest>,
) -> ResultJson<()> {
    let actor = require_admin(&headers, &state).await?;
    if request.reason.trim().is_empty() || request.reason.len() > 2000 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "赠送原因必填，最多 2000 字节",
        ));
    }
    if request.subject.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "主体不能为空"));
    }
    let mut raw = db(&state)?;
    let connection = raw
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM application_products WHERE product_id=? AND status='active')",
            [&product],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "商品不存在"));
    }
    let customer: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=? AND status='active')",
            [request.subject.trim()],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if !customer {
        return Err(error(StatusCode::BAD_REQUEST, "赠送对象必须是有效客户"));
    }
    application_commerce::manual_grant(
        &connection,
        request.subject.trim(),
        &product,
        request.expires_at,
    )?;
    application_commerce::event(
        &connection,
        request.subject.trim(),
        &actor.user_id,
        "gift",
        json!({"productId":product,"reason":request.reason.trim(),"expiresAt":request.expires_at}),
    )?;
    connection.commit().map_err(internal)?;
    Ok(Json(success("整包应用赠送已登记", ())))
}
pub(super) async fn revoke_application_entitlement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((product, subject)): Path<(String, String)>,
) -> ResultJson<()> {
    require_admin(&headers, &state).await?;
    let mut raw = db(&state)?;
    let tx = raw
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    application_commerce::revoke(&tx, &subject, &product)?;
    tx.commit().map_err(internal)?;
    Ok(Json(success("授权已撤销", ())))
}
pub(super) async fn create_application_download_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
) -> ResultJson<Value> {
    let claims = bearer_token(&headers)
        .and_then(|t| verify_license(t, &state).ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "许可证无效"))?;
    let connection = db(&state)?;
    let allowed:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM application_entitlements WHERE product_id=? AND subject=? AND status='active' AND (expires_at IS NULL OR expires_at>?))",params![product,claims.subject,Utc::now().timestamp()],|r|r.get(0)).map_err(internal)?;
    if !allowed {
        return Err(error(StatusCode::FORBIDDEN, "当前许可证主体未获此应用授权"));
    }
    let current:Option<(String,String,String)>=connection.query_row("SELECT r.submission_id,r.version,r.snapshot_json FROM application_listing_versions l JOIN application_releases r ON r.submission_id=l.release_id JOIN application_products p ON p.product_id=l.product_id WHERE l.product_id=? AND l.listing_status='listed' AND r.status='approved' AND p.status='active'",[&product],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(internal)?;
    let owned:Option<(String,String,String)>=connection.query_row("SELECT r.submission_id,r.version,r.snapshot_json FROM application_order_details a JOIN application_releases r ON r.submission_id=a.release_id WHERE a.product_id=? AND a.subject=? AND a.status IN ('completed','refund_requested') AND r.status='approved' ORDER BY a.created_at DESC LIMIT 1",params![product,claims.subject],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(internal)?;
    let (release, version, snapshot) = current
        .or(owned)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "没有可下载的已购版本"))?;
    let snapshot: Value = serde_json::from_str(&snapshot).map_err(internal)?;
    let ticket = format!("dl_{}", Uuid::new_v4().simple());
    let expires = Utc::now().timestamp() + 300;
    connection.execute("INSERT INTO application_download_tickets(ticket,release_id,subject,expires_at) VALUES(?,?,?,?)",params![ticket,release,claims.subject,expires]).map_err(internal)?;
    Ok(Json(success(
        "下载票据已签发",
        json!({"ticket":ticket,"releaseId":release,"version":version,"packageSha256":snapshot["packageSha256"],"expiresAt":expires}),
    )))
}
pub(super) async fn download_application_package(
    State(state): State<AppState>,
    Path(ticket): Path<String>,
) -> Result<Response, Failure> {
    let mut connection = db(&state)?;
    let txn = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(internal)?;
    let release:String=txn.query_row("SELECT release_id FROM application_download_tickets WHERE ticket=? AND used_at IS NULL AND expires_at>?",params![ticket,Utc::now().timestamp()],|r|r.get(0)).optional().map_err(internal)?.ok_or_else(||error(StatusCode::FORBIDDEN,"下载票据无效或已过期"))?;
    if !safe_id(&release) {
        return Err(error(StatusCode::BAD_REQUEST, "版本标识无效"));
    }
    let (buyer,product):(String,String)=txn.query_row("SELECT t.subject,r.product_id FROM application_download_tickets t JOIN application_releases r ON r.submission_id=t.release_id WHERE t.ticket=?",[&ticket],|r|Ok((r.get(0)?,r.get(1)?))).map_err(internal)?;
    if !application_commerce::entitled(&txn, &buyer, &product)? {
        return Err(error(StatusCode::FORBIDDEN, "商品权益已失效"));
    }

    let bytes = fs::read(application_package_root().join(format!("{release}.zip")))
        .map_err(|_| error(StatusCode::NOT_FOUND, "应用包不存在"))?;
    txn.execute(
        "UPDATE application_download_tickets SET used_at=? WHERE ticket=? AND used_at IS NULL",
        params![Utc::now().timestamp(), ticket],
    )
    .map_err(internal)?;
    txn.commit().map_err(internal)?;
    Ok(Response::builder()
        .header(CONTENT_TYPE, "application/zip")
        .body(Body::from(bytes))
        .map_err(internal)?)
}

pub(super) async fn application_publication_info(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product): Path<String>,
) -> ResultJson<Value> {
    let claims = bearer_token(&headers)
        .and_then(|t| verify_license(t, &state).ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "许可证无效"))?;
    let connection = db(&state)?;
    let owner: String = connection
        .query_row(
            "SELECT publisher_subject FROM application_products WHERE product_id=? AND status='active'",
            [&product],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "商品不存在"))?;
    if owner != claims.subject {
        return Err(error(StatusCode::FORBIDDEN, "不能发布其他主体的商品"));
    }
    let mut stmt = connection
        .prepare("SELECT version FROM application_releases WHERE product_id=?")
        .map_err(internal)?;
    let versions = stmt
        .query_map([product], |r| r.get::<_, String>(0))
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    let patch = versions
        .iter()
        .filter_map(|s| s.strip_prefix("1.0.")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    Ok(Json(success(
        "发布版本信息",
        json!({"nextVersion":format!("1.0.{patch}")}),
    )))
}

/// Staff may submit an offline package on behalf of an explicitly selected customer.
pub(super) async fn import_application_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> ResultJson<Value> {
    let actor = require_admin(&headers, &state).await?;
    let (mut publisher, mut product, mut bytes) = (String::new(), None, None);
    let mut commercial = Value::Null;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| error(StatusCode::BAD_REQUEST, "上传无效"))?
    {
        match field.name() {
            Some("commercial") => {
                commercial = serde_json::from_str(&field.text().await.map_err(internal)?)
                    .map_err(|_| error(StatusCode::BAD_REQUEST, "商业资料格式错误"))?
            }
            Some("publisherSubject") => publisher = field.text().await.map_err(internal)?,
            Some("productId") => {
                let value = field.text().await.map_err(internal)?;
                if !value.trim().is_empty() {
                    product = Some(value.trim().to_owned());
                }
            }
            Some("package") => {
                if bytes.is_some() {
                    return Err(error(StatusCode::BAD_REQUEST, "只能导入一个应用包"));
                }
                bytes = Some(field.bytes().await.map_err(internal)?.to_vec());
            }
            _ => return Err(error(StatusCode::BAD_REQUEST, "未知导入字段")),
        }
    }
    let publisher = publisher.trim();
    let exists: bool = db(&state)?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=? AND status='active')",
            [publisher],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if publisher != OFFICIAL_PUBLISHER && !exists {
        return Err(error(StatusCode::BAD_REQUEST, "请选择有效发布客户"));
    }
    let bytes = bytes.ok_or_else(|| error(StatusCode::BAD_REQUEST, "请选择 ZIP 应用包"))?;
    let (manifest, bundle) =
        application_protocol::read(&bytes).map_err(|m| error(StatusCode::BAD_REQUEST, &m))?;
    let name = bundle["packageName"].as_str().unwrap_or("应用包");
    let version = manifest["version"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "包缺少版本号"))?;
    // Reimporting identical bytes for the same customer/product is idempotent.
    let identity = application_protocol::digest(
        json!([
            publisher,
            product,
            application_protocol::digest(&bytes),
            commercial
        ])
        .to_string()
        .as_bytes(),
    );
    let request = ApplicationReleaseSubmission {
        submission_id: format!("sub_import_{identity}"),
        app_id: String::new(),
        app_name: name.into(),
        version: version.into(),
        status: "pending_review".into(),
        submitted_by: format!("运营导入：{}", actor.user_id),
        applicant_subject: publisher.into(),
        submitted_at: 0,
        snapshot: json!({"source":"offline-import","commercial":commercial}),
        product_id: product,
        fingerprint: String::new(),
        review_reason: String::new(),
    };
    accept_package(&state, publisher, request, bytes)
}


pub(super) async fn publisher_application_submissions(State(state): State<AppState>, headers: HeaderMap) -> ResultJson<Vec<Value>> {
    let claims = bearer_token(&headers).and_then(|t|verify_license(t,&state).ok())
        .ok_or_else(||error(StatusCode::UNAUTHORIZED,"许可证无效"))?;
    let connection = db(&state)?;
    let mut stmt = connection.prepare("SELECT submission_id FROM application_releases WHERE applicant_subject=? ORDER BY submitted_at DESC,rowid DESC").map_err(internal)?;
    let ids = stmt.query_map([&claims.subject], |r|r.get::<_,String>(0)).map_err(internal)?
        .collect::<Result<Vec<_>,_>>().map_err(internal)?;
    Ok(Json(success("发布方申请历史已读取",ids.iter().map(|id|submission(&connection,id)).collect::<Result<_,_>>()?)))
}
