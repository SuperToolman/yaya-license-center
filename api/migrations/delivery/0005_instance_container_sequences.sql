-- Instance display names may repeat; container names remain unique.
DROP TRIGGER IF EXISTS platform_instance_prevents_customer_delete;
CREATE TABLE platform_instances_new (
    instance_id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE,
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
    updated_at INTEGER NOT NULL,
    container_name TEXT NOT NULL DEFAULT '',
    environment_encrypted TEXT,
    deployment_status TEXT NOT NULL DEFAULT 'not_deployed'
        CHECK(deployment_status IN ('not_deployed','deploying','running','failed')),
    container_id TEXT,
    image_id TEXT,
    runtime_state TEXT
);
INSERT INTO platform_instances_new SELECT * FROM platform_instances;
DROP TABLE platform_instances;
ALTER TABLE platform_instances_new RENAME TO platform_instances;
CREATE INDEX platform_instance_customer ON platform_instances(customer_id);
CREATE UNIQUE INDEX platform_instance_container_name ON platform_instances(container_name);
CREATE INDEX platform_instance_bound_container ON platform_instances(container_id);
CREATE TRIGGER platform_instance_prevents_customer_delete
BEFORE DELETE ON customers
WHEN EXISTS(SELECT 1 FROM platform_instances WHERE customer_id=OLD.customer_id)
BEGIN SELECT RAISE(ABORT, 'Customer has platform instances'); END;
