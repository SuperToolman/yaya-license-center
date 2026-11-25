CREATE TABLE IF NOT EXISTS application_commercial_versions (
 id TEXT PRIMARY KEY, product_id TEXT NOT NULL, release_id TEXT NOT NULL,
 config_json TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending_review',
 submitted_by TEXT NOT NULL, reviewed_by TEXT, reason TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, reviewed_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_app_commercial_product ON application_commercial_versions(product_id,created_at);
CREATE TABLE IF NOT EXISTS application_sales_listing (product_id TEXT PRIMARY KEY, commercial_id TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS application_order_details (
 order_id TEXT PRIMARY KEY, subject TEXT NOT NULL, product_id TEXT NOT NULL, publisher_subject TEXT NOT NULL,
 release_id TEXT NOT NULL, commercial_id TEXT NOT NULL, snapshot_json TEXT NOT NULL,
 status TEXT NOT NULL, idempotency_key TEXT NOT NULL, created_at INTEGER NOT NULL,
 refund_reason TEXT NOT NULL DEFAULT '', UNIQUE(subject,idempotency_key)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_app_purchase_open ON application_order_details(subject,product_id) WHERE status IN ('pending_payment','completed','refund_requested');
CREATE TABLE IF NOT EXISTS application_entitlement_sources (
 id TEXT PRIMARY KEY, subject TEXT NOT NULL, product_id TEXT NOT NULL, kind TEXT NOT NULL,
 status TEXT NOT NULL, expires_at INTEGER, created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_app_sources_subject ON application_entitlement_sources(subject,product_id);
CREATE TABLE IF NOT EXISTS application_revenue_entries (
 id TEXT PRIMARY KEY, order_id TEXT NOT NULL, publisher_subject TEXT NOT NULL, product_id TEXT NOT NULL,
 amount_cents INTEGER NOT NULL, platform_cents INTEGER NOT NULL, gross_cents INTEGER NOT NULL,
 rate_basis_points INTEGER NOT NULL, status TEXT NOT NULL, created_at INTEGER NOT NULL,
 settlement_id TEXT, reverses_id TEXT UNIQUE
);
CREATE TABLE IF NOT EXISTS application_settlements (
 id TEXT PRIMARY KEY, publisher_subject TEXT NOT NULL, amount_cents INTEGER NOT NULL,
 status TEXT NOT NULL, actor TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS application_commerce_events (
 id TEXT PRIMARY KEY, subject TEXT NOT NULL, actor TEXT NOT NULL, kind TEXT NOT NULL,
 detail_json TEXT NOT NULL, created_at INTEGER NOT NULL
);
INSERT OR IGNORE INTO application_entitlement_sources(id,subject,product_id,kind,status,expires_at,created_at)
 SELECT 'legacy:'||subject||':'||product_id,subject,product_id,'legacy_grant',status,expires_at,created_at
 FROM application_entitlements e WHERE NOT EXISTS (
 SELECT 1 FROM application_entitlement_sources s WHERE s.subject=e.subject AND s.product_id=e.product_id);
