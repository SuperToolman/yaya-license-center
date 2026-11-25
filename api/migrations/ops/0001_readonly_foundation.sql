CREATE TABLE staff_sessions (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    credential_fingerprint TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    last_seen_at_ms INTEGER NOT NULL,
    idle_expires_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    revoked_at_ms INTEGER
);
CREATE INDEX idx_staff_sessions_user ON staff_sessions(user_id);
CREATE INDEX idx_staff_sessions_expiry ON staff_sessions(expires_at_ms);
CREATE TABLE ops_nodes (
    id TEXT PRIMARY KEY NOT NULL,
    agent_identity TEXT UNIQUE NOT NULL,
    hello_json TEXT NOT NULL,
    last_seen_at_ms INTEGER NOT NULL
);
CREATE TABLE ops_audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    actor_user_id TEXT,
    action TEXT NOT NULL,
    resource_path TEXT NOT NULL,
    outcome_status INTEGER NOT NULL,
    request_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_ops_audit_created ON ops_audit_events(created_at_ms);
