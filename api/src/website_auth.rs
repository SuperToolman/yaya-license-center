//! Website customer authentication. Never creates operation-center staff sessions.
use super::*;
use base64::Engine;
use hmac::{Hmac, Mac};
use rusqlite::TransactionBehavior;
use sha1::Sha1;

type Failure = (StatusCode, Json<Envelope<()>>);
type Reply = Result<Json<Envelope<serde_json::Value>>, Failure>;
const SESSION_TTL: i64 = 7 * 24 * 3600;
const VERIFY_TTL: i64 = 60;

pub(super) fn initialize(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("
        CREATE TABLE IF NOT EXISTS website_sms (
            mobile TEXT PRIMARY KEY, code_hash TEXT NOT NULL, expires_at INTEGER NOT NULL,
            sent_at INTEGER NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, ready INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS website_sms_limits (
            bucket TEXT PRIMARY KEY, started_at INTEGER NOT NULL, count INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS website_registration (
            token_hash TEXT PRIMARY KEY, mobile TEXT NOT NULL, expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS website_sessions (
            token_hash TEXT PRIMARY KEY, customer_id TEXT NOT NULL, mobile TEXT NOT NULL,
            expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_website_sessions_expiry ON website_sessions(expires_at);
        CREATE INDEX IF NOT EXISTS idx_website_registration_expiry ON website_registration(expires_at);
    ")
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/website/auth/send-code", post(send_code))
        .route("/api/website/auth/verify", post(verify))
        .route("/api/website/auth/register", post(register))
        .route("/api/website/auth/session", get(session))
        .route("/api/website/auth/logout", post(logout))
        .layer(DefaultBodyLimit::max(8 * 1024))
}

fn unavailable(_: impl std::fmt::Display) -> Failure {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "客户认证服务暂时不可用，请稍后重试",
    )
}
fn database(state: &AppState) -> Result<Connection, Failure> {
    let db = Connection::open(&state.database_path).map_err(unavailable)?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(unavailable)?;
    Ok(db)
}
fn hash(value: &str) -> String {
    sha256_hex(value.as_bytes())
}
fn token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn normalize_mobile(value: &str) -> Result<String, Failure> {
    let value = value.trim().replace([' ', '-'], "");
    let value = value.strip_prefix("+86").unwrap_or(&value);
    if value.len() != 11
        || !value.bytes().all(|b| b.is_ascii_digit())
        || value.as_bytes()[0] != b'1'
        || !(b'3'..=b'9').contains(&value.as_bytes()[1])
    {
        return Err(error(StatusCode::BAD_REQUEST, "请输入有效的中国大陆手机号"));
    }
    Ok(value.to_string())
}

// Never expose existence before proof of phone ownership; ambiguous legacy rows fail closed.
fn find_customer(db: &Connection, mobile: &str) -> Result<Option<CustomerRecord>, Failure> {
    let mut stmt = db.prepare("SELECT customer_id FROM customers WHERE REPLACE(REPLACE(REPLACE(contact_phone, ' ', ''), '-', ''), '+86', '') = ? LIMIT 2").map_err(unavailable)?;
    let ids = stmt
        .query_map([mobile], |r| r.get::<_, String>(0))
        .map_err(unavailable)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(unavailable)?;
    if ids.len() > 1 {
        return Err(error(
            StatusCode::CONFLICT,
            "手机号关联多个客户，请联系管理员核对客户档案",
        ));
    }
    let Some(id) = ids.first() else {
        return Ok(None);
    };
    let customer = get_customer(db, id)
        .map_err(unavailable)?
        .ok_or_else(|| unavailable("customer disappeared"))?;
    if customer.status != "active" {
        return Err(error(
            StatusCode::FORBIDDEN,
            "当前客户已被停用，请联系管理员",
        ));
    }
    Ok(Some(customer))
}

fn customer_view(c: &CustomerRecord) -> serde_json::Value {
    serde_json::json!({"customerId": c.customer_id, "companyName": c.name, "contactName": c.contact_name, "mobile": c.contact_phone})
}
fn create_session(
    db: &Connection,
    customer: &CustomerRecord,
    mobile: &str,
    now: i64,
) -> Result<serde_json::Value, Failure> {
    let session_token = token();
    db.execute("INSERT INTO website_sessions(token_hash,customer_id,mobile,expires_at,created_at) VALUES(?,?,?,?,?)",
        params![hash(&session_token), customer.customer_id, mobile, now + SESSION_TTL, now]).map_err(unavailable)?;
    Ok(
        serde_json::json!({"state":"authenticated","sessionToken":session_token,"expiresIn":SESSION_TTL,"customer":customer_view(customer)}),
    )
}

fn admit(db: &Connection, bucket: &str, maximum: i64, now: i64) -> Result<(), Failure> {
    db.execute("INSERT INTO website_sms_limits(bucket,started_at,count) VALUES(?,?,0) ON CONFLICT(bucket) DO UPDATE SET started_at=CASE WHEN started_at<=? THEN excluded.started_at ELSE started_at END, count=CASE WHEN started_at<=? THEN 0 ELSE count END",
        params![bucket,now,now-3600,now-3600]).map_err(unavailable)?;
    let count: i64 = db
        .query_row(
            "SELECT count FROM website_sms_limits WHERE bucket=?",
            [bucket],
            |r| r.get(0),
        )
        .map_err(unavailable)?;
    if count >= maximum {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "操作次数已达上限，请稍后再试",
        ));
    }
    db.execute(
        "UPDATE website_sms_limits SET count=count+1 WHERE bucket=?",
        [bucket],
    )
    .map_err(unavailable)?;
    Ok(())
}

#[derive(Deserialize)]
pub(super) struct SendCode {
    mobile: String,
}
#[derive(Deserialize)]
pub(super) struct Verify {
    mobile: String,
    code: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Register {
    registration_token: String,
    company_name: String,
    contact_name: String,
}

fn reserve_code(
    db: &mut Connection,
    mobile: &str,
    ip: &str,
    code: &str,
    now: i64,
) -> Result<String, Failure> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let sent: Option<i64> = tx
        .query_row(
            "SELECT sent_at FROM website_sms WHERE mobile=?",
            [mobile],
            |r| r.get(0),
        )
        .optional()
        .map_err(unavailable)?;
    if sent.is_some_and(|t| now - t < 60) {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "请在 60 秒后重新获取验证码",
        ));
    }
    admit(&tx, &format!("send:mobile:{mobile}"), 10, now)?;
    admit(&tx, &format!("send:ip:{ip}"), 30, now)?;
    // Store only a salted hash; raw verification codes never enter the database.
    let salt = token();
    let code_hash = format!("{salt}:{}", hash(&format!("{salt}:{mobile}:{code}")));
    tx.execute("INSERT INTO website_sms(mobile,code_hash,expires_at,sent_at,attempts,ready) VALUES(?,?,?,?,0,0) ON CONFLICT(mobile) DO UPDATE SET code_hash=excluded.code_hash,expires_at=excluded.expires_at,sent_at=excluded.sent_at,attempts=0,ready=0",
        params![mobile,code_hash,now+VERIFY_TTL,now]).map_err(unavailable)?;
    tx.execute(
        "DELETE FROM website_sms_limits WHERE started_at<?",
        [now - 3600],
    )
    .map_err(unavailable)?;
    tx.execute(
        "DELETE FROM website_registration WHERE expires_at<=?",
        [now],
    )
    .map_err(unavailable)?;
    tx.execute("DELETE FROM website_sessions WHERE expires_at<=?", [now])
        .map_err(unavailable)?;
    tx.execute("DELETE FROM website_sms WHERE sent_at<?", [now - 3600])
        .map_err(unavailable)?;
    tx.commit().map_err(unavailable)?;
    Ok(code_hash)
}

async fn send_code(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<SendCode>,
) -> Reply {
    let mobile = normalize_mobile(&body.mobile)?;
    let config = SmsConfig::from_env()?;
    let code = format!(
        "{:06}",
        u32::from_be_bytes(Uuid::new_v4().as_bytes()[..4].try_into().unwrap()) % 1_000_000
    );
    let digest = reserve_code(
        &mut database(&state)?,
        &mobile,
        &peer.ip().to_string(),
        &code,
        Utc::now().timestamp(),
    )?;
    let sent = config.send(&mobile, &code).await;
    let db = database(&state)?;
    if sent.is_err() {
        db.execute("UPDATE website_sms SET ready=0,code_hash='',expires_at=0 WHERE mobile=? AND code_hash=?", params![mobile,digest]).map_err(unavailable)?;
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "短信发送失败，请稍后重新获取验证码",
        ));
    }
    db.execute(
        "UPDATE website_sms SET ready=1 WHERE mobile=? AND code_hash=?",
        params![mobile, digest],
    )
    .map_err(unavailable)?;
    Ok(Json(success(
        "验证码已发送",
        serde_json::json!({"expiresIn":VERIFY_TTL,"retryAfter":60}),
    )))
}

fn verify_code(
    db: &mut Connection,
    mobile: &str,
    code: &str,
    now: i64,
) -> Result<serde_json::Value, Failure> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(error(StatusCode::BAD_REQUEST, "请输入 6 位短信验证码"));
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let stored: Option<(String, i64, i64, i64)> = tx
        .query_row(
            "SELECT code_hash,expires_at,attempts,ready FROM website_sms WHERE mobile=?",
            [mobile],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(unavailable)?;
    let Some((digest, expiry, attempts, ready)) = stored else {
        return Err(error(StatusCode::BAD_REQUEST, "验证码错误或已过期"));
    };
    let valid = digest
        .split_once(':')
        .is_some_and(|(salt, expected)| expected == hash(&format!("{salt}:{mobile}:{code}")));
    if expiry <= now || attempts >= 5 || ready != 1 || !valid {
        tx.execute(
            "UPDATE website_sms SET attempts=attempts+1 WHERE mobile=?",
            [mobile],
        )
        .map_err(unavailable)?;
        tx.commit().map_err(unavailable)?;
        return Err(error(StatusCode::BAD_REQUEST, "验证码错误或已过期"));
    }
    // Keep the send timestamp so successful verification does not bypass cooldown.
    tx.execute(
        "UPDATE website_sms SET ready=0,code_hash='',expires_at=0 WHERE mobile=?",
        [mobile],
    )
    .map_err(unavailable)?;
    let customer = find_customer(&tx, mobile);
    let data = match customer {
        Ok(Some(customer)) => create_session(&tx, &customer, mobile, now),
        Ok(None) => {
            let registration_token = token();
            tx.execute(
                "INSERT INTO website_registration(token_hash,mobile,expires_at) VALUES(?,?,?)",
                params![hash(&registration_token), mobile, now + 600],
            )
            .map_err(unavailable)?;
            Ok(
                serde_json::json!({"state":"registration_required","registrationToken":registration_token,"expiresIn":600}),
            )
        }
        Err(failure) => Err(failure),
    };
    tx.commit().map_err(unavailable)?;
    data
}
async fn verify(State(state): State<AppState>, Json(body): Json<Verify>) -> Reply {
    let mobile = normalize_mobile(&body.mobile)?;
    let data = verify_code(
        &mut database(&state)?,
        &mobile,
        &body.code,
        Utc::now().timestamp(),
    )?;
    Ok(Json(success("手机号已验证", data)))
}

fn register_customer(
    db: &mut Connection,
    body: &Register,
    now: i64,
) -> Result<serde_json::Value, Failure> {
    let company = body.company_name.trim();
    let contact = body.contact_name.trim();
    if company.is_empty()
        || company.chars().count() > 120
        || contact.is_empty()
        || contact.chars().count() > 60
        || body.registration_token.len() != 64
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "请填写公司名称和联系人名称，或重新验证手机号",
        ));
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let mobile: Option<String> = tx
        .query_row(
            "SELECT mobile FROM website_registration WHERE token_hash=? AND expires_at>?",
            params![hash(&body.registration_token), now],
            |r| r.get(0),
        )
        .optional()
        .map_err(unavailable)?;
    let mobile = mobile.ok_or_else(|| {
        error(
            StatusCode::UNAUTHORIZED,
            "手机号验证已失效，请重新获取验证码",
        )
    })?;
    // Recheck inside the write transaction: concurrent requests must not create duplicates.
    let customer = match find_customer(&tx, &mobile)? {
        Some(customer) => customer,
        None => {
            let id = format!("cus_{}", Uuid::new_v4().simple());
            tx.execute("INSERT INTO customers(customer_id,name,contact_name,contact_phone,contact_email,status,notes,created_at) VALUES(?,?,?,?,'','active','来源：官网手机号注册',?)", params![id,company,contact,mobile,now]).map_err(unavailable)?;
            get_customer(&tx, &id)
                .map_err(unavailable)?
                .ok_or_else(|| unavailable("customer not created"))?
        }
    };
    let data = create_session(&tx, &customer, &mobile, now)?;
    tx.execute(
        "DELETE FROM website_registration WHERE token_hash=?",
        [hash(&body.registration_token)],
    )
    .map_err(unavailable)?;
    tx.commit().map_err(unavailable)?;
    Ok(data)
}
async fn register(State(state): State<AppState>, Json(body): Json<Register>) -> Reply {
    let data = register_customer(&mut database(&state)?, &body, Utc::now().timestamp())?;
    Ok(Json(success("注册并登录成功", data)))
}

fn bearer(headers: &HeaderMap) -> Result<&str, Failure> {
    headers
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "请先使用手机号登录"))
}
fn session_customer(db: &Connection, value: &str, now: i64) -> Result<CustomerRecord, Failure> {
    let binding: Option<(String, String)> = db
        .query_row(
            "SELECT customer_id,mobile FROM website_sessions WHERE token_hash=? AND expires_at>?",
            params![hash(value), now],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(unavailable)?;
    let Some((id, mobile)) = binding else {
        return Err(error(StatusCode::UNAUTHORIZED, "登录已过期，请重新登录"));
    };
    let customer = find_customer(db, &mobile)?
        .filter(|c| c.customer_id == id)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "客户资料已变更，请重新登录"))?;
    Ok(customer)
}
async fn session(State(state): State<AppState>, headers: HeaderMap) -> Reply {
    let customer = session_customer(
        &database(&state)?,
        bearer(&headers)?,
        Utc::now().timestamp(),
    )?;
    Ok(Json(success(
        "已登录",
        serde_json::json!({"authenticated":true,"customer":customer_view(&customer)}),
    )))
}
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Reply {
    let value = bearer(&headers)?;
    database(&state)?
        .execute(
            "DELETE FROM website_sessions WHERE token_hash=?",
            [hash(value)],
        )
        .map_err(unavailable)?;
    Ok(Json(success(
        "已退出登录",
        serde_json::json!({"authenticated":false}),
    )))
}

struct SmsConfig {
    key: String,
    secret: String,
    sign: String,
    template: String,
}
pub(crate) async fn send_customer_instance_sms(mobile: &str, code: &str) -> Result<(), Failure> {
    SmsConfig::from_env()?.send(mobile,code).await.map_err(|_|error(StatusCode::SERVICE_UNAVAILABLE,"客户平台短信发送失败，请检查运营平台短信服务配置"))
}
impl SmsConfig {
    fn from_env() -> Result<Self, Failure> {
        let read = |key| {
            std::env::var(key)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| {
                    error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "短信服务尚未配置，请联系管理员",
                    )
                })
        };
        Ok(Self {
            key: read("ALIYUN_ACCESS_KEY_ID")?,
            secret: read("ALIYUN_ACCESS_KEY_SECRET")?,
            sign: read("ALIYUN_SMS_SIGN_NAME")?,
            template: read("ALIYUN_SMS_LOGIN_TEMPLATE_CODE")?,
        })
    }
    async fn send(&self, mobile: &str, code: &str) -> Result<(), ()> {
        let mut values=vec![
            ("AccessKeyId",self.key.clone()),("Action","SendSms".into()),("Format","JSON".into()),
            ("PhoneNumbers",mobile.into()),("RegionId",std::env::var("ALIYUN_SMS_REGION").unwrap_or_else(|_|"cn-hangzhou".into())),
            ("SignName",self.sign.clone()),("SignatureMethod","HMAC-SHA1".into()),("SignatureNonce",Uuid::new_v4().to_string()),("SignatureVersion","1.0".into()),
            ("TemplateCode",self.template.clone()),("TemplateParam",serde_json::json!({(std::env::var("ALIYUN_SMS_CODE_PARAM").unwrap_or_else(|_|"code".into())):code}).to_string()),
            ("Timestamp",Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),("Version","2017-05-25".into())];
        values.sort_by(|a, b| a.0.cmp(b.0));
        let canonical = values
            .iter()
            .map(|(k, v)| format!("{}={}", percent(k), percent(v)))
            .collect::<Vec<_>>()
            .join("&");
        let mut mac =
            Hmac::<Sha1>::new_from_slice(format!("{}&", self.secret).as_bytes()).map_err(|_| ())?;
        mac.update(format!("GET&%2F&{}", percent(&canonical)).as_bytes());
        values.push((
            "Signature",
            base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()),
        ));
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|_| ())?
            .get("https://dysmsapi.aliyuncs.com/")
            .query(&values)
            .send()
            .await
            .map_err(|_| ())?;
        if !response.status().is_success() {
            return Err(());
        }
        let data: serde_json::Value = response.json().await.map_err(|_| ())?;
        if data["Code"].as_str() != Some("OK") {
            return Err(());
        }
        Ok(())
    }
}
fn percent(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const PHONE: &str = "13800138000";
    const NOW: i64 = 1_800_000_000;
    fn fixture() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE customers(customer_id TEXT PRIMARY KEY,name TEXT NOT NULL,contact_name TEXT NOT NULL,contact_phone TEXT NOT NULL,contact_email TEXT NOT NULL,status TEXT NOT NULL,notes TEXT NOT NULL,created_at INTEGER NOT NULL);").unwrap();
        initialize(&db).unwrap();
        initialize(&db).unwrap();
        db
    }
    fn customer(db: &Connection, id: &str, mobile: &str, status: &str) {
        db.execute(
            "INSERT INTO customers VALUES(?,'原公司','原联系人',?,'',?,'',?)",
            params![id, mobile, status, NOW],
        )
        .unwrap();
    }
    fn seed(db: &mut Connection, mobile: &str, now: i64) {
        reserve_code(db, mobile, "test-peer", "123456", now).unwrap();
        db.execute("UPDATE website_sms SET ready=1 WHERE mobile=?", [mobile])
            .unwrap();
    }
    fn registration(ticket: &str) -> Register {
        Register {
            registration_token: ticket.into(),
            company_name: "新公司".into(),
            contact_name: "新联系人".into(),
        }
    }
    fn count(db: &Connection) -> i64 {
        db.query_row("SELECT COUNT(*) FROM customers", [], |r| r.get(0))
            .unwrap()
    }
    #[test]
    fn normalizes_phone_and_rejects_bad_inputs() {
        assert_eq!(normalize_mobile(" +86 138-0013-8000 ").unwrap(), PHONE);
        for input in [
            "",
            "12800138000",
            "1380013800",
            "13800138000abc",
            "+8513800138000",
            "１３８００１３８０００",
        ] {
            assert!(normalize_mobile(input).is_err());
        }
    }
    #[test]
    fn existing_customer_logs_in_without_creating_or_overwriting_customer() {
        let mut db = fixture();
        customer(&db, "existing", "+86 138-0013-8000", "active");
        seed(&mut db, PHONE, NOW);
        let data = verify_code(&mut db, PHONE, "123456", NOW + 1).unwrap();
        assert_eq!(data["state"], "authenticated");
        assert_eq!(data["customer"]["companyName"], "原公司");
        assert_eq!(count(&db), 1);
        let session = data["sessionToken"].as_str().unwrap();
        assert_eq!(
            session_customer(&db, session, NOW + 2).unwrap().customer_id,
            "existing"
        );
        assert!(verify_code(&mut db, PHONE, "123456", NOW + 2).is_err());
        assert!(reserve_code(&mut db, PHONE, "test-peer", "654321", NOW + 2).is_err());
        assert!(session_customer(&db, session, NOW + SESSION_TTL + 2).is_err());
    }
    #[test]
    fn new_customer_requires_profile_then_registers_in_customer_management() {
        let mut db = fixture();
        seed(&mut db, PHONE, NOW);
        let data = verify_code(&mut db, PHONE, "123456", NOW + 1).unwrap();
        assert_eq!(data["state"], "registration_required");
        assert_eq!(count(&db), 0);
        let ticket = data["registrationToken"].as_str().unwrap();
        let mut body = registration(ticket);
        body.company_name = "  客户公司  ".into();
        body.contact_name = "  小林  ".into();
        let result = register_customer(&mut db, &body, NOW + 2).unwrap();
        assert_eq!(result["customer"]["companyName"], "客户公司");
        assert_eq!(result["customer"]["contactName"], "小林");
        assert_eq!(result["customer"]["mobile"], PHONE);
        assert_eq!(count(&db), 1);
        assert!(register_customer(&mut db, &body, NOW + 3).is_err());
        let record = get_customer(&db, result["customer"]["customerId"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(record.notes, "来源：官网手机号注册");
        seed(&mut db, PHONE, NOW + 61);
        assert_eq!(
            verify_code(&mut db, PHONE, "123456", NOW + 62).unwrap()["state"],
            "authenticated"
        );
        assert_eq!(count(&db), 1);
    }
    #[test]
    fn incorrect_expired_or_unconfirmed_codes_cannot_authenticate() {
        let mut db = fixture();
        seed(&mut db, PHONE, NOW);
        for _ in 0..5 {
            assert!(verify_code(&mut db, PHONE, "999999", NOW + 1).is_err());
        }
        assert!(verify_code(&mut db, PHONE, "123456", NOW + 2).is_err());
        seed(&mut db, PHONE, NOW + 61);
        assert!(verify_code(&mut db, PHONE, "123456", NOW + 121).is_err());
        reserve_code(&mut db, PHONE, "test-peer", "123456", NOW + 180).unwrap();
        assert!(verify_code(&mut db, PHONE, "123456", NOW + 181).is_err());
        assert_eq!(count(&db), 0);
    }
    #[test]
    fn invalid_incomplete_or_expired_registration_never_creates_customer() {
        let mut db = fixture();
        seed(&mut db, PHONE, NOW);
        let data = verify_code(&mut db, PHONE, "123456", NOW + 1).unwrap();
        let ticket = data["registrationToken"].as_str().unwrap();
        let mut request = registration(ticket);
        request.company_name = " ".into();
        assert!(register_customer(&mut db, &request, NOW + 2).is_err());
        request = registration(ticket);
        request.contact_name = " ".into();
        assert!(register_customer(&mut db, &request, NOW + 2).is_err());
        assert!(register_customer(&mut db, &registration(&"a".repeat(64)), NOW + 2).is_err());
        assert!(register_customer(&mut db, &registration(ticket), NOW + 601).is_err());
        assert_eq!(count(&db), 0);
    }
    #[test]
    fn disabled_and_ambiguous_customers_are_not_allowed() {
        let mut db = fixture();
        customer(&db, "one", PHONE, "inactive");
        seed(&mut db, PHONE, NOW);
        assert_eq!(
            verify_code(&mut db, PHONE, "123456", NOW + 1)
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
        db.execute("UPDATE customers SET status='active'", [])
            .unwrap();
        customer(&db, "two", PHONE, "active");
        seed(&mut db, PHONE, NOW + 61);
        assert_eq!(
            verify_code(&mut db, PHONE, "123456", NOW + 62)
                .unwrap_err()
                .0,
            StatusCode::CONFLICT
        );
        assert_eq!(count(&db), 2);
    }
    #[test]
    fn rechecks_customer_before_registration_and_does_not_overwrite_existing_record() {
        let mut db = fixture();
        seed(&mut db, PHONE, NOW);
        let data = verify_code(&mut db, PHONE, "123456", NOW + 1).unwrap();
        customer(&db, "admin-created", PHONE, "active");
        let result = register_customer(
            &mut db,
            &registration(data["registrationToken"].as_str().unwrap()),
            NOW + 2,
        )
        .unwrap();
        assert_eq!(result["customer"]["customerId"], "admin-created");
        assert_eq!(result["customer"]["companyName"], "原公司");
        assert_eq!(count(&db), 1);
    }
    #[test]
    fn session_fails_after_disable_phone_change_or_revocation() {
        let mut db = fixture();
        customer(&db, "one", PHONE, "active");
        seed(&mut db, PHONE, NOW);
        let data = verify_code(&mut db, PHONE, "123456", NOW + 1).unwrap();
        let session = data["sessionToken"].as_str().unwrap();
        db.execute("UPDATE customers SET status='inactive'", [])
            .unwrap();
        assert!(session_customer(&db, session, NOW + 2).is_err());
        db.execute(
            "UPDATE customers SET status='active',contact_phone='13900139000'",
            [],
        )
        .unwrap();
        assert!(session_customer(&db, session, NOW + 2).is_err());
        db.execute("UPDATE customers SET contact_phone=?", [PHONE])
            .unwrap();
        assert!(session_customer(&db, session, NOW + 2).is_ok());
        db.execute(
            "DELETE FROM website_sessions WHERE token_hash=?",
            [hash(session)],
        )
        .unwrap();
        assert!(session_customer(&db, session, NOW + 2).is_err());
    }
    #[test]
    fn phone_and_peer_rate_limits_and_resend_cooldown_are_persistent() {
        let mut db = fixture();
        reserve_code(&mut db, PHONE, "peer", "123456", NOW).unwrap();
        assert_eq!(
            reserve_code(&mut db, PHONE, "peer", "123456", NOW + 59)
                .unwrap_err()
                .0,
            StatusCode::TOO_MANY_REQUESTS
        );
        for i in 1..10 {
            reserve_code(&mut db, PHONE, "peer", "123456", NOW + i * 60).unwrap();
        }
        assert_eq!(
            reserve_code(&mut db, PHONE, "peer", "123456", NOW + 600)
                .unwrap_err()
                .0,
            StatusCode::TOO_MANY_REQUESTS
        );
        reserve_code(&mut db, PHONE, "peer", "123456", NOW + 3601).unwrap();
        for i in 0..29 {
            reserve_code(&mut db, &format!("139{i:08}"), "peer2", "123456", NOW).unwrap();
        }
        reserve_code(&mut db, "13700137000", "peer2", "123456", NOW).unwrap();
        assert_eq!(
            reserve_code(&mut db, "13600136000", "peer2", "123456", NOW)
                .unwrap_err()
                .0,
            StatusCode::TOO_MANY_REQUESTS
        );
    }
    #[test]
    fn tokens_are_redacted_from_operation_audit_responses() {
        let text = sanitize_response_body(
            br#"{"data":{"sessionToken":"secret1","registrationToken":"secret2"}}"#,
        )
        .unwrap();
        assert!(!text.contains("secret1"));
        assert!(!text.contains("secret2"));
    }
    #[tokio::test]
    async fn customer_session_never_grants_operation_admin_access() {
        let mut db = fixture();
        customer(&db, "one", PHONE, "active");
        seed(&mut db, PHONE, NOW);
        let data = verify_code(&mut db, PHONE, "123456", NOW + 1).unwrap();
        let state = AppState {
            signing_key: Arc::new(EncodingKey::from_secret(b"test-only")),
            verification_key: Arc::new(DecodingKey::from_secret(b"test-only")),
            database_path: PathBuf::from("unused-test-database"),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            ops: Arc::new(crate::ops::OpsState::new(false,PathBuf::from("unused-agent.sock"))),
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!(
                "Bearer {}",
                data["sessionToken"].as_str().unwrap()
            ))
            .unwrap(),
        );
        assert!(require_admin(&headers, &state).await.is_err());
        assert!(state.sessions.read().await.is_empty());
    }
    #[test]
    fn registration_is_atomic_across_two_connections() {
        let path = std::env::temp_dir().join(format!("website-auth-{}.sqlite", Uuid::new_v4()));
        let disk = Connection::open(&path).unwrap();
        disk.execute_batch("CREATE TABLE customers(customer_id TEXT PRIMARY KEY,name TEXT NOT NULL,contact_name TEXT NOT NULL,contact_phone TEXT NOT NULL,contact_email TEXT NOT NULL,status TEXT NOT NULL,notes TEXT NOT NULL,created_at INTEGER NOT NULL);").unwrap();
        initialize(&disk).unwrap();
        drop(disk);
        let mut disk = Connection::open(&path).unwrap();
        seed(&mut disk, PHONE, NOW);
        let one = verify_code(&mut disk, PHONE, "123456", NOW + 1).unwrap();
        seed(&mut disk, PHONE, NOW + 61);
        let two = verify_code(&mut disk, PHONE, "123456", NOW + 62).unwrap();
        drop(disk);
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for data in [one, two] {
            let path = path.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                let mut db = Connection::open(path).unwrap();
                db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
                barrier.wait();
                register_customer(
                    &mut db,
                    &registration(data["registrationToken"].as_str().unwrap()),
                    NOW + 63,
                )
                .unwrap()["customer"]["customerId"]
                    .clone()
            }));
        }
        let a = handles.remove(0).join().unwrap();
        let b = handles.remove(0).join().unwrap();
        assert_eq!(a, b);
        let disk = Connection::open(&path).unwrap();
        assert_eq!(count(&disk), 1);
        drop(disk);
        std::fs::remove_file(path).unwrap();
    }
}
