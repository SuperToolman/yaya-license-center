CREATE TABLE ops_terminal_sessions (
 id TEXT PRIMARY KEY, container_id TEXT NOT NULL, actor_user_id TEXT NOT NULL,
 created_at_ms INTEGER NOT NULL, connected_at_ms INTEGER, disconnected_at_ms INTEGER,
 status TEXT NOT NULL, reason TEXT
);
CREATE INDEX ops_terminal_actor ON ops_terminal_sessions(actor_user_id,status);
CREATE TABLE ops_container_samples (
 container_id TEXT NOT NULL, captured_at_ms INTEGER NOT NULL, data TEXT NOT NULL,
 PRIMARY KEY(container_id,captured_at_ms)
);
CREATE INDEX ops_container_samples_time ON ops_container_samples(captured_at_ms);
