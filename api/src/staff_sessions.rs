//! Staff-only sessions. Website customer tokens never enter this table.
use super::{AuthenticatedUser, LoginRequest};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

const ABSOLUTE_TTL: i64 = 7 * 24 * 60 * 60 * 1000;
const IDLE_TTL: i64 = 12 * 60 * 60 * 1000;

fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub(crate) fn fingerprint(password_hash: &str, role: &str, organization: &str) -> String {
    hash(&format!("{password_hash}\0{role}\0{organization}"))
}

pub fn login(
    connection: &Connection,
    request: LoginRequest,
    now: i64,
) -> Result<(String, String), String> {
    let user: Option<(String,String,String,String)>=connection.query_row(
        "SELECT user_id,password_hash,role,organization_id FROM users WHERE username=? AND status='active'",
        [request.username.trim()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))
    ).optional().map_err(|_|"账号读取失败".to_string())?;
    let (user_id, password, role, organization) = user.ok_or("账号或密码错误")?;
    let parsed = PasswordHash::new(&password).map_err(|_| "账号或密码错误".to_string())?;
    Argon2::default()
        .verify_password(request.password.as_bytes(), &parsed)
        .map_err(|_| "账号或密码错误".to_string())?;
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    // Periodic bounded cleanup: expired tokens cannot authenticate even before deletion.
    connection.execute("DELETE FROM staff_sessions WHERE token_hash IN (SELECT token_hash FROM staff_sessions WHERE expires_at_ms<=? OR idle_expires_at_ms<=? OR revoked_at_ms IS NOT NULL LIMIT 1000)",params![now,now]).map_err(|_|"登录会话保存失败".to_string())?;
    connection.execute("INSERT INTO staff_sessions(token_hash,user_id,credential_fingerprint,created_at_ms,last_seen_at_ms,idle_expires_at_ms,expires_at_ms) VALUES(?,?,?,?,?,?,?)",
        params![hash(&token),user_id,fingerprint(&password,&role,&organization),now,now,now+IDLE_TTL,now+ABSOLUTE_TTL]).map_err(|_|"登录会话保存失败".to_string())?;
    Ok((token, user_id))
}

pub fn authenticate(
    connection: &Connection,
    token: &str,
    now: i64,
) -> Result<Option<AuthenticatedUser>, String> {
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let row: Option<(String,String,String,String,String,i64,i64)>=connection.query_row(
        "SELECT u.user_id,u.role,u.organization_id,u.password_hash,s.credential_fingerprint,s.last_seen_at_ms,s.expires_at_ms FROM staff_sessions s JOIN users u ON u.user_id=s.user_id WHERE s.token_hash=? AND s.revoked_at_ms IS NULL AND s.expires_at_ms>? AND s.idle_expires_at_ms>? AND u.status='active'",
        params![hash(token),now,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))
    ).optional().map_err(|_|"登录会话读取失败".to_string())?;
    let Some((user_id, role, organization, password, saved, last_seen, expires)) = row else {
        return Ok(None);
    };
    if fingerprint(&password, &role, &organization) != saved {
        return Ok(None);
    }
    if now - last_seen >= 60_000 {
        connection.execute("UPDATE staff_sessions SET last_seen_at_ms=?,idle_expires_at_ms=? WHERE token_hash=? AND revoked_at_ms IS NULL",params![now,(now+IDLE_TTL).min(expires),hash(token)]).map_err(|_|"登录会话更新失败".to_string())?;
    }
    Ok(Some(AuthenticatedUser {
        user_id,
        role,
        organization_id: organization,
    }))
}

pub fn revoke(connection: &Connection, token: &str, now: i64) -> Result<(), String> {
    connection
        .execute(
            "UPDATE staff_sessions SET revoked_at_ms=? WHERE token_hash=?",
            params![now, hash(token)],
        )
        .map_err(|_| "退出失败".to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use password_hash::{PasswordHasher, SaltString};
    fn fixture() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE users(user_id TEXT PRIMARY KEY, username TEXT, password_hash TEXT,role TEXT,organization_id TEXT,status TEXT);").unwrap();
        crate::ops::db::migrate(&mut connection).unwrap();
        let salt = SaltString::encode_b64(b"test-salt-at-least-sixteen-bytes").unwrap();
        let password = Argon2::default()
            .hash_password(b"password", &salt)
            .unwrap()
            .to_string();
        connection
            .execute(
                "INSERT INTO users VALUES('u1','admin',?,'platform_admin','platform','active')",
                [password],
            )
            .unwrap();
        connection
    }
    fn request() -> LoginRequest {
        LoginRequest {
            username: "admin".into(),
            password: "password".into(),
        }
    }
    #[test]
    fn hashed_sessions_expire_revoke_and_invalidate_on_role_changes() {
        let connection = fixture();
        let (token, _) = login(&connection, request(), 1000).unwrap();
        assert!(
            !connection
                .query_row("SELECT token_hash FROM staff_sessions", [], |r| r
                    .get::<_, String>(0))
                .unwrap()
                .contains(&token)
        );
        assert!(authenticate(&connection, &token, 2000).unwrap().is_some());
        assert!(
            authenticate(&connection, &token, 1000 + IDLE_TTL)
                .unwrap()
                .is_none()
        );
        connection
            .execute("UPDATE users SET role='platform_finance'", [])
            .unwrap();
        assert!(authenticate(&connection, &token, 2000).unwrap().is_none());
        let (second, _) = login(&connection, request(), 3000).unwrap();
        revoke(&connection, &second, 4000).unwrap();
        assert!(authenticate(&connection, &second, 5000).unwrap().is_none());
    }
    #[test]
    fn disabled_accounts_and_customer_tokens_never_authenticate() {
        let connection = fixture();
        let (token, _) = login(&connection, request(), 1000).unwrap();
        connection
            .execute("UPDATE users SET status='inactive'", [])
            .unwrap();
        assert!(authenticate(&connection, &token, 2000).unwrap().is_none());
        connection.execute("UPDATE users SET status='active'", []).unwrap();
        assert!(authenticate(&connection, &token, 2000).unwrap().is_none());
        assert!(
            authenticate(&connection, "website-token", 2000)
                .unwrap()
                .is_none()
        );
    }
}
