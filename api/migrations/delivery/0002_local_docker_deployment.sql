ALTER TABLE platform_instances ADD COLUMN container_name TEXT NOT NULL DEFAULT '';
ALTER TABLE platform_instances ADD COLUMN environment_encrypted TEXT;
ALTER TABLE platform_instances ADD COLUMN deployment_status TEXT NOT NULL DEFAULT 'not_deployed'
    CHECK(deployment_status IN ('not_deployed','deploying','running','failed'));
ALTER TABLE platform_instances ADD COLUMN container_id TEXT;
ALTER TABLE platform_instances ADD COLUMN image_id TEXT;
UPDATE platform_instances SET container_name='yaya-' || replace(instance_id,'-',''),node_id=NULL;
CREATE UNIQUE INDEX platform_instance_container_name ON platform_instances(container_name);
CREATE TABLE platform_deployments (
    deployment_id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES platform_instances(instance_id),
    status TEXT NOT NULL CHECK(status IN ('queued','running','succeeded','failed','interrupted')),
    stage TEXT NOT NULL DEFAULT 'queued',
    log_tail TEXT NOT NULL DEFAULT '',
    error TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    finished_at INTEGER
);
CREATE UNIQUE INDEX platform_instance_active_deployment ON platform_deployments(instance_id)
    WHERE status IN ('queued','running');
