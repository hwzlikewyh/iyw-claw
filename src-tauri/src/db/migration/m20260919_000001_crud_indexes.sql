CREATE INDEX IF NOT EXISTS idx_conversation_live_updated
    ON conversation (deleted_at, updated_at, id);
CREATE INDEX IF NOT EXISTS idx_conversation_root_updated
    ON conversation (parent_id, deleted_at, updated_at, id);
CREATE INDEX IF NOT EXISTS idx_conversation_folder_live_created
    ON conversation (folder_id, deleted_at, created_at, id);
CREATE INDEX IF NOT EXISTS idx_automation_run_conversation
    ON automation_run (conversation_id) WHERE conversation_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_channel_log_retention
    ON chat_channel_message_log (created_at, id);
CREATE INDEX IF NOT EXISTS idx_automation_run_retention
    ON automation_run (created_at, id) WHERE status <> 'running';
DROP INDEX IF EXISTS idx_conversation_folder_id;
DROP INDEX IF EXISTS idx_conversation_parent_id;
