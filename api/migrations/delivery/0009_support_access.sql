CREATE TABLE platform_support_sessions (
 id TEXT PRIMARY KEY,
 instance_id TEXT NOT NULL REFERENCES platform_instances(instance_id),
 actor_user_id TEXT NOT NULL,
 source_session_hash TEXT NOT NULL,
 code_hash TEXT NOT NULL UNIQUE,
 session_hash TEXT UNIQUE,
 reason TEXT NOT NULL,
 created_at INTEGER NOT NULL,
 code_expires_at INTEGER NOT NULL,
 expires_at INTEGER NOT NULL,
 connected_at INTEGER,
 ended_at INTEGER
);
CREATE INDEX platform_support_instance ON platform_support_sessions(instance_id,created_at);
CREATE TABLE platform_support_events (
 id TEXT PRIMARY KEY,session_id TEXT NOT NULL REFERENCES platform_support_sessions(id),
 event TEXT NOT NULL,method TEXT,path TEXT,created_at INTEGER NOT NULL
);
CREATE TABLE platform_instance_sms_attempts (
 id TEXT PRIMARY KEY,instance_id TEXT NOT NULL REFERENCES platform_instances(instance_id),
 mobile TEXT NOT NULL,created_at INTEGER NOT NULL
);
CREATE INDEX platform_instance_sms_limits ON platform_instance_sms_attempts(mobile,created_at);
