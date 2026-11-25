ALTER TABLE platform_instances ADD COLUMN runtime_state TEXT;
CREATE INDEX platform_instance_bound_container ON platform_instances(container_id);
