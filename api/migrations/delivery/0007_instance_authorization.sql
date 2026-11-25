ALTER TABLE platform_instances ADD COLUMN authorization_id TEXT REFERENCES customer_authorizations(authorization_id);
ALTER TABLE platform_instances ADD COLUMN license_id TEXT REFERENCES licenses(license_id);
ALTER TABLE platform_instances ADD COLUMN applied_license_id TEXT REFERENCES licenses(license_id);
ALTER TABLE platform_instances ADD COLUMN license_sync_status TEXT NOT NULL DEFAULT 'unbound';
ALTER TABLE platform_instances ADD COLUMN license_sync_error TEXT;
ALTER TABLE platform_instances ADD COLUMN license_synced_at INTEGER;
-- Existing instances are intentionally unbound: database records cannot prove their applied license.
CREATE TABLE platform_license_syncs (
 id TEXT PRIMARY KEY,instance_id TEXT NOT NULL REFERENCES platform_instances(instance_id),
 license_id TEXT NOT NULL REFERENCES licenses(license_id),actor_user_id TEXT NOT NULL,
 status TEXT NOT NULL,error TEXT,created_at INTEGER NOT NULL,finished_at INTEGER
);
CREATE UNIQUE INDEX platform_license_sync_active ON platform_license_syncs(instance_id) WHERE status='running';
