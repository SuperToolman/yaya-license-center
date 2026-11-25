CREATE TABLE container_revisions (
 id TEXT PRIMARY KEY, instance_id TEXT NOT NULL REFERENCES platform_instances(instance_id),
 container_id TEXT NOT NULL, image_id TEXT NOT NULL, package_id TEXT NOT NULL,
 snapshot_encrypted TEXT NOT NULL, created_at INTEGER NOT NULL, actor_id TEXT NOT NULL
);
CREATE TABLE container_recovery_points (
 id TEXT PRIMARY KEY, instance_id TEXT NOT NULL REFERENCES platform_instances(instance_id),
 revision_id TEXT NOT NULL REFERENCES container_revisions(id), status TEXT NOT NULL,
 files_json TEXT NOT NULL DEFAULT '[]', size INTEGER NOT NULL DEFAULT 0,
 created_at INTEGER NOT NULL, actor_id TEXT NOT NULL, error TEXT
);
CREATE TABLE container_version_tasks (
 id TEXT PRIMARY KEY, instance_id TEXT NOT NULL REFERENCES platform_instances(instance_id),
 kind TEXT NOT NULL, status TEXT NOT NULL, stage TEXT NOT NULL,
 source_container_id TEXT NOT NULL, source_revision_id TEXT,
 recovery_point_id TEXT, target_recovery_point_id TEXT,
 target_package_id TEXT, target_container_id TEXT,
 resources_json TEXT NOT NULL DEFAULT '{}', request_encrypted TEXT NOT NULL,
 actor_id TEXT NOT NULL, created_at INTEGER NOT NULL, finished_at INTEGER,
 error TEXT, recovery_error TEXT
);
CREATE UNIQUE INDEX container_version_task_active ON container_version_tasks(instance_id)
 WHERE status IN ('queued','running','recovering','attention');
CREATE INDEX container_revision_instance ON container_revisions(instance_id,created_at);
CREATE INDEX container_recovery_instance ON container_recovery_points(instance_id,created_at);
