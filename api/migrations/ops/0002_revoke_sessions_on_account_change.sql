CREATE TRIGGER IF NOT EXISTS revoke_staff_sessions_on_account_change
AFTER UPDATE OF password_hash, role, organization_id, status ON users
WHEN OLD.password_hash IS NOT NEW.password_hash
  OR OLD.role IS NOT NEW.role
  OR OLD.organization_id IS NOT NEW.organization_id
  OR OLD.status IS NOT NEW.status
BEGIN
    UPDATE staff_sessions SET revoked_at_ms=CAST(strftime('%s','now') AS INTEGER)*1000
    WHERE user_id=NEW.user_id AND revoked_at_ms IS NULL;
END;
