PRAGMA foreign_keys = ON;

DROP VIEW IF EXISTS sync_backlog;
DROP VIEW IF EXISTS workspace_activity_summary;
DROP VIEW IF EXISTS agent_thread_summary;

DROP TABLE IF EXISTS local_kv_state;
DROP TABLE IF EXISTS file_blobs;
DROP TABLE IF EXISTS app_event_log;
DROP TABLE IF EXISTS background_jobs;
DROP TABLE IF EXISTS sync_tombstones;
DROP TABLE IF EXISTS offline_sync_records;
DROP TABLE IF EXISTS document_chunks_fts;
DROP TABLE IF EXISTS document_chunks;
DROP TABLE IF EXISTS local_files;
DROP TABLE IF EXISTS local_workspaces;
DROP TABLE IF EXISTS agent_memory_fts;
DROP TABLE IF EXISTS agent_memory_items;
DROP TABLE IF EXISTS agent_tool_calls;
DROP TABLE IF EXISTS agent_messages_fts;
DROP TABLE IF EXISTS agent_messages;
DROP TABLE IF EXISTS agent_threads;

CREATE TABLE agent_threads (
    id INTEGER PRIMARY KEY,
    external_thread_id TEXT NOT NULL UNIQUE,
    workspace_slug TEXT NOT NULL,
    title TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    model TEXT NOT NULL,
    goal TEXT,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'archived', 'failed', 'paused')),
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_agent_threads_workspace ON agent_threads(workspace_slug);
CREATE INDEX idx_agent_threads_status ON agent_threads(status);
CREATE INDEX idx_agent_threads_created_at ON agent_threads(created_at DESC);

CREATE TABLE agent_messages (
    id INTEGER PRIMARY KEY,
    thread_id INTEGER NOT NULL REFERENCES agent_threads(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('system', 'user', 'assistant', 'tool')),
    turn_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    token_count INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}',
    content_length INTEGER GENERATED ALWAYS AS (length(content)) VIRTUAL,
    message_preview TEXT GENERATED ALWAYS AS (substr(replace(content, char(10), ' '), 1, 96)) VIRTUAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (thread_id, turn_index, role)
);

CREATE INDEX idx_agent_messages_thread_id ON agent_messages(thread_id);
CREATE INDEX idx_agent_messages_role ON agent_messages(role);
CREATE INDEX idx_agent_messages_content_length ON agent_messages(content_length DESC);
CREATE INDEX idx_agent_messages_created_at ON agent_messages(created_at DESC);

CREATE VIRTUAL TABLE agent_messages_fts USING fts5(
    role,
    content,
    metadata,
    content='agent_messages',
    content_rowid='id'
);

CREATE TRIGGER agent_messages_fts_ai AFTER INSERT ON agent_messages BEGIN
    INSERT INTO agent_messages_fts(rowid, role, content, metadata)
    VALUES (new.id, new.role, new.content, new.metadata);
END;

CREATE TRIGGER agent_messages_fts_ad AFTER DELETE ON agent_messages BEGIN
    INSERT INTO agent_messages_fts(agent_messages_fts, rowid, role, content, metadata)
    VALUES ('delete', old.id, old.role, old.content, old.metadata);
END;

CREATE TRIGGER agent_messages_fts_au AFTER UPDATE ON agent_messages BEGIN
    INSERT INTO agent_messages_fts(agent_messages_fts, rowid, role, content, metadata)
    VALUES ('delete', old.id, old.role, old.content, old.metadata);

    INSERT INTO agent_messages_fts(rowid, role, content, metadata)
    VALUES (new.id, new.role, new.content, new.metadata);
END;

CREATE TABLE agent_tool_calls (
    id INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES agent_messages(id) ON DELETE CASCADE,
    tool_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    result_text TEXT,
    status TEXT NOT NULL DEFAULT 'ok'
        CHECK (status IN ('ok', 'failed', 'timeout', 'cancelled')),
    elapsed_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_agent_tool_calls_message_id ON agent_tool_calls(message_id);
CREATE INDEX idx_agent_tool_calls_tool_name ON agent_tool_calls(tool_name);
CREATE INDEX idx_agent_tool_calls_status ON agent_tool_calls(status);

CREATE TABLE agent_memory_items (
    id INTEGER PRIMARY KEY,
    thread_id INTEGER REFERENCES agent_threads(id) ON DELETE SET NULL,
    memory_key TEXT NOT NULL,
    memory_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    body TEXT NOT NULL,
    embedding_model TEXT,
    embedding_ref TEXT,
    importance REAL NOT NULL DEFAULT 0,
    tags TEXT NOT NULL DEFAULT '[]',
    source_json TEXT NOT NULL DEFAULT '{}',
    body_length INTEGER GENERATED ALWAYS AS (length(body)) VIRTUAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_agent_memory_items_key ON agent_memory_items(memory_key);
CREATE INDEX idx_agent_memory_items_type ON agent_memory_items(memory_type);
CREATE INDEX idx_agent_memory_items_importance ON agent_memory_items(importance DESC);
CREATE INDEX idx_agent_memory_items_body_length ON agent_memory_items(body_length DESC);

CREATE VIRTUAL TABLE agent_memory_fts USING fts5(
    summary,
    body,
    tags,
    content='agent_memory_items',
    content_rowid='id'
);

CREATE TRIGGER agent_memory_fts_ai AFTER INSERT ON agent_memory_items BEGIN
    INSERT INTO agent_memory_fts(rowid, summary, body, tags)
    VALUES (new.id, new.summary, new.body, new.tags);
END;

CREATE TRIGGER agent_memory_fts_ad AFTER DELETE ON agent_memory_items BEGIN
    INSERT INTO agent_memory_fts(agent_memory_fts, rowid, summary, body, tags)
    VALUES ('delete', old.id, old.summary, old.body, old.tags);
END;

CREATE TRIGGER agent_memory_fts_au AFTER UPDATE ON agent_memory_items BEGIN
    INSERT INTO agent_memory_fts(agent_memory_fts, rowid, summary, body, tags)
    VALUES ('delete', old.id, old.summary, old.body, old.tags);

    INSERT INTO agent_memory_fts(rowid, summary, body, tags)
    VALUES (new.id, new.summary, new.body, new.tags);
END;

CREATE TABLE local_workspaces (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    root_path TEXT NOT NULL,
    app_version TEXT NOT NULL,
    last_opened_at TEXT,
    settings_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_local_workspaces_last_opened ON local_workspaces(last_opened_at DESC);

CREATE TABLE local_files (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER NOT NULL REFERENCES local_workspaces(id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL,
    file_kind TEXT NOT NULL,
    mime_type TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    content_hash TEXT NOT NULL,
    is_pinned INTEGER NOT NULL DEFAULT 0 CHECK (is_pinned IN (0, 1)),
    last_read_at TEXT,
    modified_at TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    path_lc TEXT GENERATED ALWAYS AS (lower(relative_path)) VIRTUAL,
    UNIQUE (workspace_id, relative_path)
);

CREATE INDEX idx_local_files_workspace_id ON local_files(workspace_id);
CREATE INDEX idx_local_files_kind ON local_files(file_kind);
CREATE INDEX idx_local_files_path_lc ON local_files(path_lc);

CREATE TABLE document_chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES local_files(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    token_count INTEGER NOT NULL,
    embedding_model TEXT,
    embedding_ref TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    body_length INTEGER GENERATED ALWAYS AS (length(body)) VIRTUAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (file_id, chunk_index)
);

CREATE INDEX idx_document_chunks_file_id ON document_chunks(file_id);
CREATE INDEX idx_document_chunks_token_count ON document_chunks(token_count DESC);
CREATE INDEX idx_document_chunks_body_length ON document_chunks(body_length DESC);

CREATE VIRTUAL TABLE document_chunks_fts USING fts5(
    title,
    body,
    content='document_chunks',
    content_rowid='id'
);

CREATE TRIGGER document_chunks_fts_ai AFTER INSERT ON document_chunks BEGIN
    INSERT INTO document_chunks_fts(rowid, title, body)
    VALUES (new.id, new.title, new.body);
END;

CREATE TRIGGER document_chunks_fts_ad AFTER DELETE ON document_chunks BEGIN
    INSERT INTO document_chunks_fts(document_chunks_fts, rowid, title, body)
    VALUES ('delete', old.id, old.title, old.body);
END;

CREATE TRIGGER document_chunks_fts_au AFTER UPDATE ON document_chunks BEGIN
    INSERT INTO document_chunks_fts(document_chunks_fts, rowid, title, body)
    VALUES ('delete', old.id, old.title, old.body);

    INSERT INTO document_chunks_fts(rowid, title, body)
    VALUES (new.id, new.title, new.body);
END;

CREATE TABLE offline_sync_records (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER NOT NULL REFERENCES local_workspaces(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    local_id TEXT NOT NULL,
    server_id TEXT,
    sync_state TEXT NOT NULL CHECK (sync_state IN ('clean', 'dirty', 'conflict', 'deleted', 'failed')),
    version INTEGER NOT NULL DEFAULT 1,
    payload_json TEXT NOT NULL,
    last_error TEXT,
    last_synced_at TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (workspace_id, entity_type, local_id)
);

CREATE INDEX idx_offline_sync_records_state ON offline_sync_records(sync_state);
CREATE INDEX idx_offline_sync_records_updated_at ON offline_sync_records(updated_at DESC);

CREATE TABLE sync_tombstones (
    workspace_id INTEGER NOT NULL REFERENCES local_workspaces(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    deleted_at TEXT NOT NULL,
    reason TEXT,
    PRIMARY KEY (workspace_id, entity_type, entity_id)
) WITHOUT ROWID;

CREATE TABLE background_jobs (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER REFERENCES local_workspaces(id) ON DELETE CASCADE,
    queue_name TEXT NOT NULL,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'retrying')),
    priority INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    payload_json TEXT NOT NULL,
    last_error TEXT,
    locked_until TEXT,
    scheduled_at TEXT NOT NULL,
    finished_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_background_jobs_status_priority ON background_jobs(status, priority DESC);
CREATE INDEX idx_background_jobs_scheduled_at ON background_jobs(scheduled_at);
CREATE INDEX idx_background_jobs_queue ON background_jobs(queue_name);

CREATE TABLE app_event_log (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER REFERENCES local_workspaces(id) ON DELETE SET NULL,
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('debug', 'info', 'warn', 'error')),
    actor TEXT NOT NULL,
    message TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    event_date TEXT GENERATED ALWAYS AS (date(created_at)) VIRTUAL
);

CREATE INDEX idx_app_event_log_type ON app_event_log(event_type);
CREATE INDEX idx_app_event_log_severity ON app_event_log(severity);
CREATE INDEX idx_app_event_log_created_at ON app_event_log(created_at DESC);
CREATE INDEX idx_app_event_log_date ON app_event_log(event_date);

CREATE TABLE file_blobs (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER NOT NULL REFERENCES local_workspaces(id) ON DELETE CASCADE,
    blob_kind TEXT NOT NULL,
    file_name TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    content BLOB NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_file_blobs_workspace_id ON file_blobs(workspace_id);
CREATE INDEX idx_file_blobs_kind ON file_blobs(blob_kind);

CREATE TABLE local_kv_state (
    workspace_id INTEGER NOT NULL REFERENCES local_workspaces(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (workspace_id, namespace, key)
) WITHOUT ROWID;

CREATE VIEW agent_thread_summary AS
SELECT
    t.id,
    t.workspace_slug,
    t.title,
    t.status,
    COUNT(DISTINCT m.id) AS message_count,
    COUNT(DISTINCT tc.id) AS tool_call_count,
    COUNT(DISTINCT mem.id) AS memory_count,
    MAX(m.created_at) AS last_message_at
FROM agent_threads t
LEFT JOIN agent_messages m ON m.thread_id = t.id
LEFT JOIN agent_tool_calls tc ON tc.message_id = m.id
LEFT JOIN agent_memory_items mem ON mem.thread_id = t.id
GROUP BY t.id;

CREATE VIEW workspace_activity_summary AS
SELECT
    w.id,
    w.slug,
    w.display_name,
    COUNT(DISTINCT f.id) AS file_count,
    COUNT(DISTINCT c.id) AS chunk_count,
    COUNT(DISTINCT j.id) AS job_count,
    COUNT(DISTINCT e.id) AS event_count,
    COUNT(DISTINCT s.id) AS sync_record_count,
    w.last_opened_at
FROM local_workspaces w
LEFT JOIN local_files f ON f.workspace_id = w.id
LEFT JOIN document_chunks c ON c.file_id = f.id
LEFT JOIN background_jobs j ON j.workspace_id = w.id
LEFT JOIN app_event_log e ON e.workspace_id = w.id
LEFT JOIN offline_sync_records s ON s.workspace_id = w.id
GROUP BY w.id;

CREATE VIEW sync_backlog AS
SELECT
    w.slug AS workspace_slug,
    s.entity_type,
    s.sync_state,
    COUNT(*) AS record_count,
    MAX(s.updated_at) AS newest_update_at
FROM offline_sync_records s
JOIN local_workspaces w ON w.id = s.workspace_id
WHERE s.sync_state <> 'clean'
GROUP BY w.slug, s.entity_type, s.sync_state;
