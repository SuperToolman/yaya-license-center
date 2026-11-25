use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(connection)
}

pub fn migrate(connection: &mut Connection) -> Result<(), String> {
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS ops_schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL, checksum TEXT NOT NULL, applied_at_ms INTEGER NOT NULL);").map_err(|e|e.to_string())?;
    let foundation = include_str!("../../migrations/ops/0001_readonly_foundation.sql");
    let revoke = include_str!("../../migrations/ops/0002_revoke_sessions_on_account_change.sql");
    // A development build accidentally appended this trigger to migration 1.
    // Recognize only that exact historical SQL, never arbitrary checksum changes.
    let combined = foundation.replace(
        "CREATE TABLE ops_nodes",
        &format!(
            "{}CREATE TABLE ops_nodes",
            revoke.replace("IF NOT EXISTS ", "")
        ),
    );
    for (version, name, sql) in [
        (1, "readonly_foundation", foundation),
        (2, "revoke_sessions_on_account_change", revoke),
        (
            3,
            "container_actions",
            include_str!("../../migrations/ops/0003_container_actions.sql"),
        ),
        (
            4,
            "action_reconciliation",
            include_str!("../../migrations/ops/0004_action_reconciliation.sql"),
        ),
        (
            5,
            "container_observability",
            include_str!("../../migrations/ops/0005_container_observability.sql"),
        ),
        (
            6,
            "terminal_exec_identity",
            include_str!("../../migrations/ops/0006_terminal_exec_identity.sql"),
        ),
        (
            7,
            "container_creation",
            include_str!("../../migrations/ops/0007_container_creation.sql"),
        ),
    ] {
        let checksum = sql_checksum(sql);
        let saved: Option<String> = transaction
            .query_row(
                "SELECT checksum FROM ops_schema_migrations WHERE version=?",
                [version],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match saved {
            Some(saved) if !matches_checksum(&saved, sql) => {
                if version == 1 && matches_checksum(&saved, &combined) {
                    transaction
                        .execute(
                            "UPDATE ops_schema_migrations SET checksum=? WHERE version=1",
                            [&checksum],
                        )
                        .map_err(|e| e.to_string())?;
                } else {
                    return Err(format!(
                        "Operations migration checksum mismatch (version {version})"
                    ));
                }
            }
            Some(_) => {}
            None => {
                transaction.execute_batch(sql).map_err(|e| e.to_string())?;
                transaction
                    .execute(
                        "INSERT INTO ops_schema_migrations VALUES (?,?,?,?)",
                        params![
                            version,
                            name,
                            checksum,
                            chrono::Utc::now().timestamp_millis()
                        ],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    transaction.commit().map_err(|e| e.to_string())
}

fn sql_checksum(sql: &str) -> String {
    format!("{:x}", Sha256::digest(sql.replace("\r\n", "\n").as_bytes()))
}

fn matches_checksum(saved: &str, sql: &str) -> bool {
    let lf = sql.replace("\r\n", "\n");
    saved == sql_checksum(&lf)
        || saved == format!("{:x}", Sha256::digest(lf.replace('\n', "\r\n").as_bytes()))
}

pub async fn blocking<T: Send + 'static>(
    path: std::path::PathBuf,
    work: impl FnOnce(&mut Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || {
        let mut connection = open(&path).map_err(|e| e.to_string())?;
        work(&mut connection)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_is_atomic_idempotent_and_verifies_checksum() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE users(user_id TEXT PRIMARY KEY);INSERT INTO users VALUES('existing');").unwrap();
        migrate(&mut connection).unwrap();
        migrate(&mut connection).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM users", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM ops_schema_migrations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            7
        );
        connection
            .execute("UPDATE ops_schema_migrations SET checksum='tampered'", [])
            .unwrap();
        assert!(migrate(&mut connection).is_err());
    }

    #[test]
    fn upgrades_original_and_interim_databases_without_losing_data() {
        let foundation = include_str!("../../migrations/ops/0001_readonly_foundation.sql");
        let trigger =
            include_str!("../../migrations/ops/0002_revoke_sessions_on_account_change.sql");
        let combined = foundation.replace(
            "CREATE TABLE ops_nodes",
            &format!(
                "{}CREATE TABLE ops_nodes",
                trigger.replace("IF NOT EXISTS ", "")
            ),
        );
        assert_eq!(
            sql_checksum(foundation),
            "d69386284f927c51f7a1bacbe9e0afca3bf7de35e84c7db4dbdd1874c83e72e8"
        );
        assert_eq!(
            sql_checksum(&combined),
            "f009fae7579937f01a0e8819b082099d0088bee385f81e6dd26f5d71d8c69ab6"
        );
        for historical in [foundation.to_string(), combined] {
            let historical = historical.replace("\r\n", "\n");
            for crlf in [false, true] {
                let mut db = Connection::open_in_memory().unwrap();
                db.execute_batch("CREATE TABLE users(user_id TEXT PRIMARY KEY,password_hash TEXT,role TEXT,organization_id TEXT,status TEXT); INSERT INTO users VALUES('u','hash','admin','platform','active'); CREATE TABLE ops_schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL,checksum TEXT NOT NULL,applied_at_ms INTEGER NOT NULL);").unwrap();
                db.execute_batch(&historical).unwrap();
                let bytes = if crlf {
                    historical.replace('\n', "\r\n")
                } else {
                    historical.clone()
                };
                db.execute(
                    "INSERT INTO ops_schema_migrations VALUES(1,'readonly_foundation',?,123)",
                    [format!("{:x}", Sha256::digest(bytes.as_bytes()))],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO staff_sessions VALUES('token','u','fingerprint',1,1,2,3,NULL)",
                    [],
                )
                .unwrap();
                migrate(&mut db).unwrap();
                migrate(&mut db).unwrap();
                assert_eq!(
                    db.query_row("SELECT count(*) FROM staff_sessions", [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    1
                );
                db.execute("UPDATE users SET status='inactive' WHERE user_id='u'", [])
                    .unwrap();
                assert!(
                    db.query_row(
                        "SELECT revoked_at_ms IS NOT NULL FROM staff_sessions",
                        [],
                        |r| r.get::<_, bool>(0)
                    )
                    .unwrap()
                );
                db.execute(
                    "UPDATE ops_schema_migrations SET checksum='unexpected-change' WHERE version=2",
                    [],
                )
                .unwrap();
                assert!(migrate(&mut db).is_err());
            }
        }
    }
}
