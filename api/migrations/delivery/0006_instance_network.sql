ALTER TABLE platform_instances ADD COLUMN network_mode TEXT NOT NULL DEFAULT 'bridge'
 CHECK(network_mode IN ('bridge','macvlan'));
ALTER TABLE platform_instances ADD COLUMN network_name TEXT NOT NULL DEFAULT 'bridge';
