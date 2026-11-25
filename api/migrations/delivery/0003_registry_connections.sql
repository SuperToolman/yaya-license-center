CREATE TABLE platform_registries (
    registry_id TEXT PRIMARY KEY,
    alias TEXT NOT NULL COLLATE NOCASE UNIQUE,
    url TEXT NOT NULL,
    username TEXT NOT NULL DEFAULT '',
    credentials TEXT,
    mirrors TEXT NOT NULL DEFAULT '[]',
    is_default INTEGER NOT NULL DEFAULT 0 CHECK(is_default IN (0,1))
);
CREATE UNIQUE INDEX platform_registry_default ON platform_registries(is_default) WHERE is_default=1;
INSERT INTO platform_registries(registry_id,alias,url,is_default) VALUES('dockerhub','DockerHub','https://hub.docker.com',1);
