use super::*;
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};

pub(super) type Variables = BTreeMap<String, String>;
const MANAGED: &[&str] = &[
    "APP_ENV",
    "APP_HOST",
    "APP_PORT",
    "DATABASE_URL",
    "VALKEY_URL",
    "BACKEND_API_BASE_URL",
    "YAYA_LICENSE_PUBLIC_KEY_PATH",
    "YAYA_AGENT_V2_API_URL",
    "YAYA_AGENT_V2_CORDIS_HOST",
    "YAYA_AGENT_V2_OFFICE_PATH",
    "YAYA_AGENT_V2_EXECUTION_URL",
    "YAYA_EXECUTION_ROOT",
    "YAYA_UPLOAD_DIR",
    "YAYA_SETTINGS_PATH",
    "YAYA_VALKEY_SETTINGS_PATH",
    "YAYA_IDENTITY_SETTINGS_PATH",
    "YAYA_NOTIFICATION_SETTINGS_PATH",
    "YAYA_LICENSE_SETTINGS_PATH",
    "YAYA_SKILLS_PATH",
    "CORS_ALLOWED_ORIGINS",
    "AUTH_COOKIE_SECURE",
    "YAYA_PLATFORM_ADMIN_MOBILE",
    "YAYA_PLATFORM_INSTANCE_ID",
];

fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
pub(super) fn parse(text: &str) -> Result<Variables, Failure> {
    if text.len() > 32768 {
        return Err(bad("环境变量不得超过 32 KiB"));
    }
    let mut result = Variables::new();
    let pattern = regex::Regex::new(r"^[A-Z][A-Z0-9_]{0,79}$").unwrap();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| bad("环境变量格式须为每行 KEY=value"))?;
        let key = key.trim();
        if !pattern.is_match(key)
            || value.contains(['\0', '\r', '\n'])
            || MANAGED.contains(&key)
            || ["LD_PRELOAD", "LD_LIBRARY_PATH", "NODE_OPTIONS", "PATH"].contains(&key)
        {
            return Err(bad(
                "环境变量名称不合法，或试图覆盖由平台维护的端口、路径和内部连接配置",
            ));
        }
        if result.insert(key.into(), value.into()).is_some() {
            return Err(bad("环境变量名称重复"));
        }
    }
    Ok(result)
}
pub(super) fn prepare(
    old: Option<Variables>,
    text: Option<&str>,
    domain: &str,
) -> Result<Variables, Failure> {
    let mut vars = old.unwrap_or_default();
    for (key, value) in [
        ("APP_ENV", "production"),
        ("APP_HOST", "0.0.0.0"),
        ("APP_PORT", "8788"),
        ("POSTGRES_DB", "yaya_low_code"),
        ("POSTGRES_USER", "yaya"),
        ("VALKEY_CACHE_TTL_HOURS", "8"),
        ("BACKEND_API_BASE_URL", "http://127.0.0.1:8788"),
        (
            "YAYA_LICENSE_PUBLIC_KEY_PATH",
            "/run/secrets/license-public.pem",
        ),
        ("YAYA_AGENT_V2_API_URL", "http://127.0.0.1:8788"),
        (
            "YAYA_AGENT_V2_CORDIS_HOST",
            "/app/agent-v2/cordis-host/src/host.mjs",
        ),
        (
            "YAYA_AGENT_V2_OFFICE_PATH",
            "/usr/local/bin/yaya-agent-v2-office",
        ),
        ("YAYA_AGENT_V2_EXECUTION_URL", "http://127.0.0.1:8791"),
        ("YAYA_EXECUTION_ROOT", "/var/lib/yaya/executions"),
        ("YAYA_UPLOAD_DIR", "/var/lib/yaya/uploads"),
        ("YAYA_SETTINGS_PATH", "/var/lib/yaya/config/database.json"),
        (
            "YAYA_VALKEY_SETTINGS_PATH",
            "/var/lib/yaya/config/valkey.json",
        ),
        (
            "YAYA_IDENTITY_SETTINGS_PATH",
            "/var/lib/yaya/config/identity.json",
        ),
        (
            "YAYA_NOTIFICATION_SETTINGS_PATH",
            "/var/lib/yaya/config/notifications.json",
        ),
        (
            "YAYA_LICENSE_SETTINGS_PATH",
            "/var/lib/yaya/config/license.json",
        ),
        ("YAYA_SKILLS_PATH", "/var/lib/yaya/skills"),
    ] {
        vars.entry(key.into()).or_insert(value.into());
    }
    for key in [
        "POSTGRES_PASSWORD",
        "VALKEY_PASSWORD",
        "AUTH_TOKEN_SECRET",
        "BACKEND_INTERNAL_TOKEN",
        "YAYA_AGENT_V2_WORKER_SECRET",
    ] {
        vars.entry(key.into()).or_insert_with(secret);
    }
    vars.entry("YAYA_BYOM_ENCRYPTION_KEY".into())
        .or_insert_with(|| B64.encode(Sha256::digest(secret().as_bytes())));
    if let Some(text) = text {
        vars.extend(parse(text)?);
    }
    let identifier = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]{0,62}$").unwrap();
    for key in ["POSTGRES_DB", "POSTGRES_USER"] {
        if !identifier.is_match(&vars[key]) {
            return Err(bad("POSTGRES_DB 和 POSTGRES_USER 必须为有效数据库标识符"));
        }
    }
    for key in [
        "POSTGRES_PASSWORD",
        "VALKEY_PASSWORD",
        "AUTH_TOKEN_SECRET",
        "BACKEND_INTERNAL_TOKEN",
        "YAYA_AGENT_V2_WORKER_SECRET",
    ] {
        if vars[key].len() < 16 {
            return Err(bad(
                "数据库、缓存、认证和内部服务密钥至少需要 16 个字符；留空配置可使用自动生成值",
            ));
        }
    }
    if B64
        .decode(&vars["YAYA_BYOM_ENCRYPTION_KEY"])
        .ok()
        .is_none_or(|v| v.len() != 32)
    {
        return Err(bad(
            "YAYA_BYOM_ENCRYPTION_KEY 必须为 Base64 编码的 32 字节密钥",
        ));
    }
    let mut db = reqwest::Url::parse("postgres://localhost").unwrap();
    db.set_username(&vars["POSTGRES_USER"])
        .map_err(|_| bad("数据库用户名不合法"))?;
    db.set_password(Some(&vars["POSTGRES_PASSWORD"]))
        .map_err(|_| bad("数据库密码不合法"))?;
    db.set_host(Some("127.0.0.1")).map_err(db_error)?;
    db.set_port(Some(5432))
        .map_err(|_| bad("数据库端口不合法"))?;
    db.set_path(&vars["POSTGRES_DB"]);
    vars.insert("DATABASE_URL".into(), db.into());
    let mut redis = reqwest::Url::parse("redis://127.0.0.1:6379/0").unwrap();
    redis
        .set_password(Some(&vars["VALKEY_PASSWORD"]))
        .map_err(|_| bad("缓存密码不合法"))?;
    vars.insert("VALKEY_URL".into(), redis.into());
    let execution_secret = vars["YAYA_AGENT_V2_WORKER_SECRET"].clone();
    vars.entry("YAYA_EXECUTION_SECRET".into())
        .or_insert(execution_secret);
    // Without a public domain, Web proxies API requests on the same origin.
    // Keep a nonempty restrictive CORS value to satisfy the package contract.
    let public = reqwest::Url::parse(if domain.trim().is_empty() {
        "http://localhost"
    } else {
        domain
    }).map_err(|_| bad("请配置有效的 HTTP/HTTPS 域名"))?;
    vars.insert(
        "CORS_ALLOWED_ORIGINS".into(),
        public.origin().ascii_serialization(),
    );
    vars.insert(
        "AUTH_COOKIE_SECURE".into(),
        (public.scheme() == "https").to_string(),
    );
    Ok(vars)
}
pub(super) fn bind_customer(db: &Connection, customer: &str, id: &str, vars: &mut Variables) -> Result<(), Failure> {
    let phone: String = db.query_row("SELECT contact_phone FROM customers WHERE customer_id=?", [customer], |r|r.get(0)).map_err(db_error)?;
    let phone = phone.trim().replace([' ', '-'], "");
    let phone = phone.strip_prefix("+86").unwrap_or(&phone);
    if !regex::Regex::new(r"^1[3-9][0-9]{9}$").unwrap().is_match(phone) {
        return Err(bad("客户联系人电话须为有效中国大陆手机号，才能初始化客户平台管理员"));
    }
    vars.insert("YAYA_PLATFORM_ADMIN_MOBILE".into(), phone.into());
    vars.insert("YAYA_PLATFORM_INSTANCE_ID".into(), id.into());
    Ok(())
}
pub(super) fn key(path: &FilePath, create: bool) -> Result<Vec<u8>, Failure> {
    let location = path
        .parent()
        .unwrap_or(FilePath::new("."))
        .join(".platform-environment.key");
    if create && !location.exists() {
        let bytes = Sha256::digest(secret().as_bytes());
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&location) {
            Ok(mut f) => {
                f.write_all(&bytes).map_err(db_error)?;
                f.sync_all().map_err(db_error)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(db_error(e)),
        }
    }
    let bytes = fs::read(location).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "平台环境变量加密密钥缺失或不可读，请恢复数据库配套的密钥文件",
        )
    })?;
    if bytes.len() != 32 {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "平台环境变量加密密钥无效",
        ));
    }
    Ok(bytes)
}
pub(super) fn encrypt(db: &FilePath, id: &str, vars: &Variables) -> Result<String, Failure> {
    let cipher = Aes256Gcm::new_from_slice(&key(db, true)?).map_err(db_error)?;
    let random = Uuid::new_v4();
    let nonce = &random.as_bytes()[..12];
    let raw = serde_json::to_vec(vars).map_err(db_error)?;
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: &raw,
                aad: id.as_bytes(),
            },
        )
        .map_err(|_| bad("环境变量加密失败"))?;
    Ok(serde_json::json!({"nonce":B64.encode(nonce),"ciphertext":B64.encode(encrypted),"keys":vars.keys().collect::<Vec<_>>(),"format":1}).to_string())
}
pub(super) fn decrypt(db: &FilePath, id: &str, text: &str) -> Result<Variables, Failure> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(db_error)?;
    let nonce = B64
        .decode(value["nonce"].as_str().unwrap_or(""))
        .map_err(db_error)?;
    if nonce.len() != 12 {
        return Err(bad("环境变量加密记录无效"));
    }
    let ciphertext = B64
        .decode(value["ciphertext"].as_str().unwrap_or(""))
        .map_err(db_error)?;
    let cipher = Aes256Gcm::new_from_slice(&key(db, false)?).map_err(db_error)?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: id.as_bytes(),
            },
        )
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "环境变量解密失败，请检查配套加密密钥是否正确",
            )
        })?;
    serde_json::from_slice(&plain).map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_variables_and_encrypts_with_instance_bound_identity() {
        assert!(parse("A=1\nA=2").is_err());
        assert!(parse("APP_PORT=1").is_err());
        assert!(parse("bad=x").is_err());
        let env = prepare(
            None,
            Some("ALIYUN_ACCESS_KEY_ID=example-key\nCUSTOM_VALUE=a=b"),
            "https://customer.example.com",
        )
        .unwrap();
        assert_eq!(env["CUSTOM_VALUE"], "a=b");
        assert_eq!(env["POSTGRES_DB"], "yaya_low_code");
        assert_eq!(env["POSTGRES_USER"], "yaya");
        let without_domain = prepare(None, None, "").unwrap();
        assert_eq!(without_domain["CORS_ALLOWED_ORIGINS"], "http://localhost");
        assert_eq!(without_domain["AUTH_COOKIE_SECURE"], "false");
        let other = prepare(None, None, "https://another.example.com").unwrap();
        for name in ["POSTGRES_PASSWORD", "VALKEY_PASSWORD", "AUTH_TOKEN_SECRET", "BACKEND_INTERNAL_TOKEN", "YAYA_AGENT_V2_WORKER_SECRET", "YAYA_BYOM_ENCRYPTION_KEY"] {
            assert_ne!(env[name], other[name], "each instance needs its own {name}");
        }
        assert!(env["POSTGRES_PASSWORD"].len() >= 32);
        let root = std::env::temp_dir().join(Uuid::new_v4().to_string());
        fs::create_dir(&root).unwrap();
        let db = root.join("db.sqlite3");
        let text = encrypt(&db, "instance-a", &env).unwrap();
        assert!(!text.contains("example-key"));
        assert_eq!(decrypt(&db, "instance-a", &text).unwrap(), env);
        assert!(decrypt(&db, "instance-b", &text).is_err());
        let edited = prepare(
            Some(env.clone()),
            Some("CUSTOM_VALUE=new"),
            "https://new.example.com",
        )
        .unwrap();
        assert_eq!(edited["POSTGRES_PASSWORD"], env["POSTGRES_PASSWORD"]);
        for name in ["VALKEY_PASSWORD", "AUTH_TOKEN_SECRET", "BACKEND_INTERNAL_TOKEN", "YAYA_AGENT_V2_WORKER_SECRET", "YAYA_BYOM_ENCRYPTION_KEY", "YAYA_EXECUTION_SECRET"] {
            assert_eq!(edited[name], env[name], "editing must retain {name}");
        }
        fs::remove_dir_all(root).unwrap();
    }
}
