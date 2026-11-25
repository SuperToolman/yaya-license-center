CREATE TABLE platform_builds (
    build_id TEXT PRIMARY KEY,
    package_id TEXT NOT NULL REFERENCES platform_packages(package_id),
    package_sha256 TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('queued','running','building','checking','succeeded','failed','interrupted')),
    stage TEXT NOT NULL DEFAULT 'queued',
    image_tag TEXT,
    image_digest TEXT,
    error TEXT,
    log_tail TEXT NOT NULL DEFAULT '',
    worker_id TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    heartbeat_at INTEGER
);
CREATE UNIQUE INDEX platform_build_active_package ON platform_builds(package_id)
WHERE status IN ('queued','running','building','checking');
CREATE INDEX platform_build_created ON platform_builds(created_at DESC);
CREATE TABLE platform_instances (
    instance_id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    customer_id TEXT NOT NULL REFERENCES customers(customer_id),
    package_id TEXT NOT NULL REFERENCES platform_packages(package_id),
    node_id TEXT REFERENCES ops_nodes(id),
    web_port INTEGER NOT NULL CHECK(web_port BETWEEN 1024 AND 65535),
    api_port INTEGER NOT NULL CHECK(api_port BETWEEN 1024 AND 65535 AND api_port != web_port),
    public_url TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','archived')),
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX platform_instance_customer ON platform_instances(customer_id);
CREATE TABLE platform_delivery_events (
    id INTEGER PRIMARY KEY,
    resource_id TEXT NOT NULL,
    action TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TRIGGER platform_instance_prevents_customer_delete
BEFORE DELETE ON customers
WHEN EXISTS(SELECT 1 FROM platform_instances WHERE customer_id=OLD.customer_id)
BEGIN SELECT RAISE(ABORT, 'Customer has platform instances'); END;
