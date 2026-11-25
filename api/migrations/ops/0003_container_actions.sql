CREATE TABLE ops_container_actions (
    id TEXT PRIMARY KEY,
    container_id TEXT NOT NULL,
    action TEXT NOT NULL,
    actor_user_id TEXT NOT NULL,
    status TEXT NOT NULL,
    error TEXT,
    created_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER
);
CREATE UNIQUE INDEX ops_container_actions_active ON ops_container_actions(container_id) WHERE status='running';
