ALTER TABLE ops_terminal_sessions ADD COLUMN exec_id TEXT;
ALTER TABLE ops_terminal_sessions ADD COLUMN shell_pid TEXT;
ALTER TABLE ops_terminal_sessions ADD COLUMN cleanup_result TEXT;
