CREATE TABLE ops_container_creations (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, image_id TEXT NOT NULL, container_id TEXT,
 actor_user_id TEXT NOT NULL, status TEXT NOT NULL, error TEXT,
 created_at_ms INTEGER NOT NULL, finished_at_ms INTEGER
);
CREATE UNIQUE INDEX ops_container_creation_name ON ops_container_creations(name) WHERE status IN ('running','interrupted');
