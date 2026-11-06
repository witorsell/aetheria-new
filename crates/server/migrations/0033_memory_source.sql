ALTER TABLE memory_chunks ADD COLUMN source_content TEXT NOT NULL DEFAULT '';

-- old chunks cannot be checked against the text that produced them
DELETE FROM memory_chunks;

CREATE TRIGGER invalidate_edited_memory
AFTER UPDATE OF content, visible, deleted ON messages
BEGIN
    DELETE FROM memory_chunks WHERE message_id = NEW.id;
END;

CREATE TRIGGER invalidate_deleted_memory
AFTER DELETE ON messages
BEGIN
    DELETE FROM memory_chunks WHERE message_id = OLD.id;
END;
