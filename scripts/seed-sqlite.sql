PRAGMA foreign_keys = ON;

INSERT INTO local_workspaces (id, slug, display_name, root_path, app_version, last_opened_at, settings_json, created_at) VALUES
(1, 'sabiql-dev', 'sabiql development', '/Users/example/Projects/sabiql', '1.12.0-dev', datetime('now', '-10 minutes'), '{"theme":"dark","sidebarWidth":32,"resultPageSize":100}', datetime('now', '-45 days')),
(2, 'agent-lab', 'Agent Lab', '/Users/example/AgentLab', '0.42.1', datetime('now', '-1 hour'), '{"memoryCompaction":true,"retrievalLimit":12,"model":"gpt-5.2"}', datetime('now', '-30 days')),
(3, 'research-notes', 'Research Notes', '/Users/example/Documents/research-notes', '3.8.0', datetime('now', '-2 days'), '{"offlineMode":true,"syncWifiOnly":true}', datetime('now', '-120 days')),
(4, 'mobile-cache', 'Mobile Cache Mirror', '/Users/example/Library/Application Support/MobileCache', '8.4.2', datetime('now', '-5 hours'), '{"lastSync":"2026-05-18T23:00:00Z","conflictPolicy":"manual"}', datetime('now', '-90 days')),
(5, 'browser-extension', 'Browser Extension State', '/Users/example/Library/Application Support/BrowserExtension', '2.19.0', datetime('now', '-30 minutes'), '{"historyRetentionDays":90,"ftsEnabled":true}', datetime('now', '-180 days')),
(6, 'desktop-agent', 'Desktop Agent State', '/Users/example/Library/Application Support/DesktopAgent', '0.9.7', datetime('now', '-4 minutes'), '{"localFirst":true,"jobWorkers":4}', datetime('now', '-14 days')),
(7, 'docs-cache', 'Docs Cache', '/Users/example/.cache/docs', '5.1.0', datetime('now', '-7 hours'), '{"chunkSize":1200,"embeddingModel":"text-embedding-3-small"}', datetime('now', '-60 days')),
(8, 'crash-reports', 'Crash Reports', '/Users/example/Library/Logs/AgentCrashes', '0.9.7', datetime('now', '-12 hours'), '{"uploadEnabled":false,"symbolication":"local"}', datetime('now', '-20 days'));

INSERT INTO agent_threads (id, external_thread_id, workspace_slug, title, agent_name, model, goal, status, metadata, created_at) VALUES
(1, 'thread-ai-001', 'sabiql-dev', 'Investigate SQLite inspector rendering', 'codex-local', 'gpt-5.2', 'Review a local SQLite database produced by an AI agent and identify result-pane UX issues for long text cells.', 'active', '{"workspace":"sabiql","source":"sqlite-seed"}', datetime('now', '-3 days')),
(2, 'thread-ai-002', 'research-notes', 'Summarize customer research notes', 'research-agent', 'gpt-5.2-mini', 'Turn long interview transcripts into compact product insights while preserving citations.', 'archived', '{"workspace":"research","source":"sqlite-seed"}', datetime('now', '-8 days')),
(3, 'thread-ai-003', 'desktop-agent', 'Generate migration plan for local app state', 'planner-agent', 'gpt-5.2', 'Draft a stepwise migration plan from JSON files to SQLite-backed local memory.', 'active', '{"workspace":"desktop-agent","source":"sqlite-seed"}', datetime('now', '-1 day'));

INSERT INTO agent_messages (id, thread_id, role, turn_index, content, content_hash, token_count, metadata, created_at) VALUES
(1, 1, 'system', 0, 'You are a local coding agent. Keep answers compact, inspect real files before proposing changes, and preserve user edits. When browsing SQLite-backed memory stores, prefer read-only investigation unless explicitly asked to mutate data.', lower(hex(randomblob(16))), 42, '{"kind":"instruction"}', datetime('now', '-3 days')),
(2, 1, 'user', 1, 'I opened a SQLite database used by an AI desktop app. Some cells contain very long prompts, Markdown summaries, and JSON tool outputs. The grid view truncates everything, so I cannot tell whether the data is valid without copying values out to another tool. Please analyze what a better TUI experience should look like.', lower(hex(randomblob(16))), 58, '{"kind":"prompt","contains_long_text":true}', datetime('now', '-3 days', '+2 minutes')),
(3, 1, 'assistant', 2, 'The result pane should keep the grid dense and scannable, then provide a focused detail view for the selected cell. A long prompt or memory body should not expand row height inside the table because that makes navigation unstable. A modal can preserve line breaks, wrap paragraphs, support vertical scrolling, and optionally switch into a structured JSON view when the content parses as JSON.\n\nThe interaction should be read-first. Inline edit is a separate action with stronger guardrails, because opening a long prompt for inspection should never feel like the user is about to modify application memory. This is especially important for SQLite files because they are often live local state for an agent, not disposable reporting replicas.\n\nA useful first version is a cell detail modal with four rendering modes: plain text, JSON, empty/NULL, and binary or invalid UTF-8. Plain text keeps Markdown readable without trying to become a full Markdown renderer. JSON uses the existing JSONB modal behavior where possible. Binary values should show size and a short hex preview instead of corrupt text.', lower(hex(randomblob(16))), 178, '{"kind":"analysis","contains_newlines":true}', datetime('now', '-3 days', '+5 minutes')),
(4, 2, 'user', 1, 'Here are raw interview notes. Please preserve the important wording.\n\nParticipant A said the tool feels fast until they hit a cell that contains a whole generated answer. At that point the table stops being useful because the preview is only a narrow slice of the value.\n\nParticipant B said they expect Enter to mean open or inspect, not edit. They were worried about accidentally changing memory records while exploring an agent database.\n\nParticipant C said JSON payloads should be formatted automatically, because a single-line object with escaped newlines is unreadable in a terminal grid.', lower(hex(randomblob(16))), 111, '{"kind":"research_notes","contains_newlines":true}', datetime('now', '-8 days')),
(5, 2, 'assistant', 2, 'Three product implications stand out:\n\n1. Keep table navigation stable. Do not let long values resize rows.\n2. Add an inspect action for the selected cell. Treat it as read-only by default.\n3. Reuse existing structured JSON affordances instead of inventing a separate viewer.\n\nThe most concrete UI gap is not SQLite-specific. SQLite makes it visible because local AI tools commonly store prompts, transcripts, tool outputs, and memory summaries in TEXT columns, but PostgreSQL users with JSONB and TEXT columns will benefit from the same design.', lower(hex(randomblob(16))), 103, '{"kind":"summary","contains_markdown":true}', datetime('now', '-8 days', '+7 minutes')),
(6, 3, 'user', 1, 'Draft a migration plan. The current app writes one JSON file per conversation. Each file contains messages, tool call results, compact summaries, and long-term memories. We want a single SQLite database so the desktop app can query recent work quickly, but we need to preserve debuggability.', lower(hex(randomblob(16))), 55, '{"kind":"planning_prompt"}', datetime('now', '-1 day')),
(7, 3, 'assistant', 2, 'Recommended migration plan:\n\nPhase 1 keeps the JSON files as source of truth and builds a SQLite mirror. The mirror should include conversations, messages, tool calls, and memory items. Every imported row stores the original file path and byte offset where possible, so debugging can still jump back to the old artifact.\n\nPhase 2 changes reads to prefer SQLite while writes still update JSON first. A background verifier compares row counts, message counts, and content hashes. Any mismatch disables SQLite reads for that workspace.\n\nPhase 3 moves writes to SQLite and exports periodic JSON snapshots for recovery. At that point the UI should include a long-cell inspector because prompts, responses, and memory bodies are no longer convenient to inspect as files.', lower(hex(randomblob(16))), 145, '{"kind":"plan","contains_newlines":true}', datetime('now', '-1 day', '+9 minutes'));

WITH RECURSIVE seq(n) AS (
    VALUES(4)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 303
)
INSERT INTO agent_threads (id, external_thread_id, workspace_slug, title, agent_name, model, goal, status, metadata, created_at)
SELECT
    n,
    printf('thread-ai-%03d', n),
    CASE n % 8 WHEN 0 THEN 'sabiql-dev' WHEN 1 THEN 'agent-lab' WHEN 2 THEN 'research-notes' WHEN 3 THEN 'mobile-cache' WHEN 4 THEN 'browser-extension' WHEN 5 THEN 'desktop-agent' WHEN 6 THEN 'docs-cache' ELSE 'crash-reports' END,
    CASE n % 5
        WHEN 0 THEN 'Debug local memory compaction'
        WHEN 1 THEN 'Plan offline cache reconciliation'
        WHEN 2 THEN 'Summarize retrieved document chunks'
        WHEN 3 THEN 'Investigate failed tool execution'
        ELSE 'Generate prompt rewrite candidates'
    END || ' #' || n,
    CASE n % 4 WHEN 0 THEN 'codex-local' WHEN 1 THEN 'research-agent' WHEN 2 THEN 'planner-agent' ELSE 'sync-agent' END,
    CASE n % 3 WHEN 0 THEN 'gpt-5.2' WHEN 1 THEN 'gpt-5.2-mini' ELSE 'local-embedding-reranker' END,
    'Synthetic but realistic AI agent thread with long prompts, tool traces, memory writes, and SQLite local state references.',
    CASE n % 6 WHEN 0 THEN 'failed' WHEN 1 THEN 'archived' ELSE 'active' END,
    '{"source":"sqlite-seed","shape":"ai-agent-thread","containsLongText":true}',
    datetime('now', '-' || n || ' hours')
FROM seq;

WITH RECURSIVE threads(n) AS (
    VALUES(4)
    UNION ALL
    SELECT n + 1 FROM threads WHERE n < 303
),
turns(turn_index, role) AS (
    VALUES(0, 'system'), (1, 'user'), (2, 'assistant'), (3, 'tool')
)
INSERT INTO agent_messages (id, thread_id, role, turn_index, content, content_hash, token_count, metadata, created_at)
SELECT
    1000 + (threads.n * 10) + turns.turn_index,
    threads.n,
    turns.role,
    turns.turn_index,
    CASE turns.role
        WHEN 'system' THEN 'You are running inside a local desktop agent. Use the SQLite memory store as read-only context unless the user explicitly asks for mutation. Preserve source IDs, include concise reasoning, and avoid rewriting unrelated state.'
        WHEN 'user' THEN 'Please inspect the recent memory rows for workspace ' || threads.n || '. I need to understand why the agent kept recalling an outdated instruction. The database may include long prompts, compressed summaries, JSON tool results, and offline sync records. Keep the investigation grounded in the stored rows and quote only short excerpts.'
        WHEN 'assistant' THEN 'I found three likely causes in the local memory store.\n\nFirst, the high-importance memory row was updated after the newer correction, so retrieval score favored the stale summary. Second, the conversation summary preserved the old instruction in a dense paragraph, which made it hard to visually distinguish from the correction. Third, the background compaction job retried after a transient file lock and wrote a merged summary with both versions.\n\nA better database client flow would let the user open the long memory body in a modal, inspect related tool calls, and compare timestamps without copying values into another editor.'
        ELSE '{"tool":"memory_lookup","status":"ok","rows":[{"table":"agent_memory_items","matches":3},{"table":"background_jobs","matches":1}],"notes":"Long JSON tool payload stored as text for local debugging."}'
    END,
    lower(hex(randomblob(16))),
    CASE turns.role WHEN 'assistant' THEN 164 WHEN 'user' THEN 78 WHEN 'tool' THEN 52 ELSE 38 END,
    '{"generated":true,"dataset":"ai-agent-memory"}',
    datetime('now', '-' || threads.n || ' hours', '+' || turns.turn_index || ' minutes')
FROM threads
JOIN turns;

INSERT INTO agent_messages_fts(rowid, role, content, metadata)
SELECT id, role, content, metadata FROM agent_messages;

INSERT INTO agent_tool_calls (message_id, tool_name, arguments_json, result_text, status, elapsed_ms, created_at)
SELECT
    1000 + (id * 10) + 2,
    CASE id % 4 WHEN 0 THEN 'memory_lookup' WHEN 1 THEN 'sqlite_query' WHEN 2 THEN 'document_search' ELSE 'job_status' END,
    '{"workspaceId":' || (1 + (id % 8)) || ',"query":"stale instruction retrieval","limit":20,"readOnly":true}',
    'Tool result for generated thread ' || id || '.\n\nRows inspected:\n- agent_memory_items: long body with conflicting instruction\n- document_chunks_fts: matching prompt fragments\n- background_jobs: retrying compaction job\n\nThe raw payload is intentionally multi-line so a result cell needs a detail view.',
    CASE id % 9 WHEN 0 THEN 'failed' ELSE 'ok' END,
    100 + (id * 17),
    datetime('now', '-' || id || ' hours', '+4 minutes')
FROM agent_threads
WHERE id >= 4;

INSERT INTO agent_tool_calls (message_id, tool_name, arguments_json, result_text, status, elapsed_ms, created_at) VALUES
(3, 'sqlite_schema_scan', '{"database":"/tmp/agent-memory.sqlite3","tables":["agent_messages","agent_memory_items"],"read_only":true}', 'Found local-first SQLite tables. Long text candidates: agent_messages.content, agent_memory_items.body, agent_tool_calls.result_text. JSON candidates: metadata, arguments_json, tags.', 'ok', 184, datetime('now', '-3 days', '+4 minutes')),
(5, 'summarize_transcript', '{"mode":"extractive","preserve_quotes":true,"max_points":5}', 'The clearest repeated observation is that long cells need a separate reading surface. Participants did not ask for a spreadsheet-like expanded row; they asked for a focused view that keeps the grid usable.', 'ok', 922, datetime('now', '-8 days', '+6 minutes')),
(7, 'migration_risk_check', '{"source":"conversation-json","target":"sqlite","checks":["hash","row-count","foreign-key"]}', 'Risk notes:\n- Message ordering must remain stable across imports.\n- Tool result payloads may contain escaped JSON inside text.\n- Memory summaries are short, but memory bodies can exceed several thousand characters.', 'ok', 441, datetime('now', '-1 day', '+8 minutes'));

WITH RECURSIVE seq(n) AS (
    VALUES(1)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 1203
)
INSERT INTO agent_memory_items (id, thread_id, memory_key, memory_type, summary, body, embedding_model, embedding_ref, importance, tags, source_json, created_at)
SELECT
    n,
    1 + (n % 303),
    printf('memory.%04d.%s', n, CASE n % 4 WHEN 0 THEN 'instruction' WHEN 1 THEN 'preference' WHEN 2 THEN 'tool-result' ELSE 'summary' END),
    CASE n % 4 WHEN 0 THEN 'instruction' WHEN 1 THEN 'user_preference' WHEN 2 THEN 'tool_trace' ELSE 'conversation_summary' END,
    'Generated memory item ' || n || ' with prompt-like long text.',
    'Memory item #' || n || '\n\nContext:\nThe user was working inside a local SQLite-backed agent database. The row stores a compact but still long memory body containing preferences, recent task state, and references to tool outputs. This is representative of desktop AI agents that keep memory in a local file so the app can recover context quickly without a server process.\n\nObserved behavior:\n- Retrieval favored this row because the importance score was high.\n- The body contains paragraphs, bullets, and JSON-looking snippets.\n- The content is too long for a table cell but too important to hide entirely.\n\nStored payload excerpt:\n{"workspace":"sabiql","reason":"exercise long text cells","sequence":' || n || ',"needsModal":true}\n\nFollow-up:\nA TUI should open this cell in a read-focused modal with wrapping, scrolling, and JSON-aware formatting when possible.',
    CASE n % 2 WHEN 0 THEN 'text-embedding-3-large' ELSE 'text-embedding-3-small' END,
    'embedding://memory/' || n,
    (n % 100) / 100.0,
    CASE n % 5
        WHEN 0 THEN '["ai-memory","instruction","long-text"]'
        WHEN 1 THEN '["ai-memory","preference","sqlite"]'
        WHEN 2 THEN '["tool-trace","json","debug"]'
        WHEN 3 THEN '["conversation-summary","markdown"]'
        ELSE '["retrieval","local-state"]'
    END,
    '{"seed":"sqlite-realistic","record":' || n || '}',
    datetime('now', '-' || n || ' minutes')
FROM seq;

INSERT INTO agent_memory_fts(rowid, summary, body, tags)
SELECT id, summary, body, tags FROM agent_memory_items;

INSERT INTO local_kv_state (workspace_id, namespace, key, value_json, updated_at)
SELECT
    w.id,
    ns.namespace,
    kv.key,
    '{"value":"' || ns.namespace || '.' || kv.key || '","workspace":' || w.id || ',"updatedBy":"sqlite-seed"}',
    datetime('now', '-' || ((w.id * 7) + kv.ord) || ' minutes')
FROM local_workspaces w
JOIN (
    SELECT 'window' AS namespace UNION ALL SELECT 'editor' UNION ALL SELECT 'memory' UNION ALL SELECT 'sync'
) ns
JOIN (
    SELECT 1 AS ord, 'lastSelectedTable' AS key
    UNION ALL SELECT 2, 'scrollOffset'
    UNION ALL SELECT 3, 'expandedPanels'
    UNION ALL SELECT 4, 'lastSearch'
    UNION ALL SELECT 5, 'featureFlags'
) kv;

WITH RECURSIVE seq(n) AS (
    VALUES(1)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 800
)
INSERT INTO local_files (id, workspace_id, relative_path, file_kind, mime_type, size_bytes, content_hash, is_pinned, last_read_at, modified_at, metadata_json)
SELECT
    n,
    1 + (n % 8),
    CASE n % 6
        WHEN 0 THEN 'memory/thread-' || printf('%03d', n) || '.md'
        WHEN 1 THEN 'prompts/system-' || printf('%03d', n) || '.txt'
        WHEN 2 THEN 'tool-results/result-' || printf('%03d', n) || '.json'
        WHEN 3 THEN 'documents/note-' || printf('%03d', n) || '.md'
        WHEN 4 THEN 'cache/page-' || printf('%03d', n) || '.html'
        ELSE 'logs/run-' || printf('%03d', n) || '.log'
    END,
    CASE n % 6 WHEN 0 THEN 'memory' WHEN 1 THEN 'prompt' WHEN 2 THEN 'tool_result' WHEN 3 THEN 'note' WHEN 4 THEN 'web_cache' ELSE 'log' END,
    CASE n % 6 WHEN 2 THEN 'application/json' WHEN 4 THEN 'text/html' ELSE 'text/plain' END,
    1024 + (n * 137),
    lower(hex(randomblob(16))),
    CASE WHEN n % 17 = 0 THEN 1 ELSE 0 END,
    datetime('now', '-' || (n % 240) || ' minutes'),
    datetime('now', '-' || (n % 45) || ' days'),
    '{"source":"local-app","hasLongText":true,"sequence":' || n || '}'
FROM seq;

WITH chunk_index(idx) AS (
    VALUES(0), (1), (2)
)
INSERT INTO document_chunks (id, file_id, chunk_index, title, body, token_count, embedding_model, embedding_ref, metadata_json, created_at)
SELECT
    ((f.id - 1) * 3) + chunk_index.idx + 1,
    f.id,
    chunk_index.idx,
    'Chunk ' || chunk_index.idx || ' from ' || f.relative_path,
    'Document chunk from ' || f.relative_path || '\n\nThis chunk simulates local-first AI application data. It may be a prompt template, retrieved document passage, browser capture, markdown note, crash analysis, or tool result. SQLite-backed apps often keep this text directly in a TEXT column and build FTS indexes beside it.\n\nImportant details:\n- workspace_id=' || f.workspace_id || '\n- file_kind=' || f.file_kind || '\n- chunk_index=' || chunk_index.idx || '\n\nThe content is intentionally verbose so table-cell truncation is visible. Searching for phrases like stale instruction, offline sync, prompt template, crash report, or retrieved document should hit the FTS virtual table.',
    180 + ((f.id + chunk_index.idx) % 500),
    CASE f.id % 2 WHEN 0 THEN 'text-embedding-3-small' ELSE 'text-embedding-3-large' END,
    'embedding://' || f.workspace_id || '/' || f.id || '/' || chunk_index.idx,
    '{"sourceFile":"' || f.relative_path || '","chunkIndex":' || chunk_index.idx || ',"fts":true}',
    datetime('now', '-' || (f.id + chunk_index.idx) || ' minutes')
FROM local_files f
JOIN chunk_index;

INSERT INTO document_chunks_fts(rowid, title, body)
SELECT id, title, body FROM document_chunks;

WITH RECURSIVE seq(n) AS (
    VALUES(1)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 8000
)
INSERT INTO offline_sync_records (id, workspace_id, entity_type, local_id, server_id, sync_state, version, payload_json, last_error, last_synced_at, updated_at)
SELECT
    n,
    1 + (n % 8),
    CASE n % 5 WHEN 0 THEN 'memory' WHEN 1 THEN 'message' WHEN 2 THEN 'document' WHEN 3 THEN 'setting' ELSE 'job' END,
    printf('local-%04d', n),
    CASE WHEN n % 4 = 0 THEN NULL ELSE printf('srv-%04d', n) END,
    CASE n % 9 WHEN 0 THEN 'conflict' WHEN 1 THEN 'dirty' WHEN 2 THEN 'deleted' WHEN 3 THEN 'failed' ELSE 'clean' END,
    1 + (n % 12),
    '{"localOnly":' || CASE WHEN n % 4 = 0 THEN 'true' ELSE 'false' END || ',"record":' || n || ',"body":"offline cache payload with JSON stored as TEXT"}',
    CASE WHEN n % 9 IN (0, 3) THEN 'Conflict while merging local prompt edits with remote memory summary. Manual resolution required.' ELSE NULL END,
    CASE WHEN n % 4 = 0 THEN NULL ELSE datetime('now', '-' || (n % 72) || ' hours') END,
    datetime('now', '-' || n || ' minutes')
FROM seq;

INSERT INTO sync_tombstones (workspace_id, entity_type, entity_id, deleted_at, reason)
SELECT
    1 + (n % 8),
    CASE n % 4 WHEN 0 THEN 'memory' WHEN 1 THEN 'message' WHEN 2 THEN 'document' ELSE 'job' END,
    printf('deleted-%04d', n),
    datetime('now', '-' || n || ' hours'),
    CASE n % 3 WHEN 0 THEN 'remote deletion won conflict' WHEN 1 THEN 'local cleanup' ELSE 'retention policy' END
FROM (WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 500) SELECT n FROM seq);

WITH RECURSIVE seq(n) AS (
    VALUES(1)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 2000
)
INSERT INTO background_jobs (id, workspace_id, queue_name, job_type, status, priority, attempts, max_attempts, payload_json, last_error, locked_until, scheduled_at, finished_at, created_at)
SELECT
    n,
    1 + (n % 8),
    CASE n % 3 WHEN 0 THEN 'default' WHEN 1 THEN 'sync' ELSE 'agent' END,
    CASE n % 7 WHEN 0 THEN 'compact_memory' WHEN 1 THEN 'sync_workspace' WHEN 2 THEN 'embed_document' WHEN 3 THEN 'refresh_fts' WHEN 4 THEN 'upload_crash_report' WHEN 5 THEN 'summarize_thread' ELSE 'vacuum_database' END,
    CASE n % 10 WHEN 0 THEN 'failed' WHEN 1 THEN 'retrying' WHEN 2 THEN 'running' WHEN 3 THEN 'queued' ELSE 'succeeded' END,
    n % 5,
    n % 4,
    5,
    '{"jobId":' || n || ',"source":"sqlite-seed","payloadKind":"local-background-work","longTextCandidate":true}',
    CASE WHEN n % 10 IN (0, 1) THEN 'Job failed while processing a long prompt. The retry log includes enough text to require a cell detail modal.\n\nStack trace excerpt:\n  at compactMemory(memory_id=' || n || ')\n  at writeSummary(sqlite://local-agent-state)\nReason: database is locked during concurrent read.' ELSE NULL END,
    CASE WHEN n % 10 = 2 THEN datetime('now', '+5 minutes') ELSE NULL END,
    datetime('now', '-' || (n % 300) || ' minutes'),
    CASE WHEN n % 10 >= 4 THEN datetime('now', '-' || (n % 250) || ' minutes') ELSE NULL END,
    datetime('now', '-' || n || ' minutes')
FROM seq;

WITH RECURSIVE seq(n) AS (
    VALUES(1)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 20000
)
INSERT INTO app_event_log (id, workspace_id, event_type, severity, actor, message, payload_json, created_at)
SELECT
    n,
    CASE WHEN n % 13 = 0 THEN NULL ELSE 1 + (n % 8) END,
    CASE n % 12
        WHEN 0 THEN 'app.opened'
        WHEN 1 THEN 'query.executed'
        WHEN 2 THEN 'memory.retrieved'
        WHEN 3 THEN 'tool.started'
        WHEN 4 THEN 'tool.finished'
        WHEN 5 THEN 'sync.conflict'
        WHEN 6 THEN 'fts.rebuild'
        WHEN 7 THEN 'job.retry'
        WHEN 8 THEN 'file.cached'
        WHEN 9 THEN 'prompt.saved'
        WHEN 10 THEN 'crash.detected'
        ELSE 'settings.changed'
    END,
    CASE n % 20 WHEN 0 THEN 'error' WHEN 1 THEN 'warn' WHEN 2 THEN 'debug' ELSE 'info' END,
    CASE n % 4 WHEN 0 THEN 'user' WHEN 1 THEN 'assistant' WHEN 2 THEN 'worker' ELSE 'system' END,
    'Event ' || n || ' generated from local SQLite app state',
    '{"event":' || n || ',"workspace":' || (1 + (n % 8)) || ',"details":"JSON payload stored in TEXT column for debugging local state"}',
    datetime('now', '-' || n || ' seconds')
FROM seq;

WITH RECURSIVE seq(n) AS (
    VALUES(1)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 150
)
INSERT INTO file_blobs (id, workspace_id, blob_kind, file_name, content_type, byte_size, content, content_hash, created_at)
SELECT
    n,
    1 + (n % 8),
    CASE n % 4 WHEN 0 THEN 'thumbnail' WHEN 1 THEN 'favicon' WHEN 2 THEN 'minidump' ELSE 'attachment' END,
    'blob-' || printf('%03d', n) || CASE n % 4 WHEN 0 THEN '.png' WHEN 1 THEN '.ico' WHEN 2 THEN '.dmp' ELSE '.bin' END,
    CASE n % 4 WHEN 0 THEN 'image/png' WHEN 1 THEN 'image/x-icon' ELSE 'application/octet-stream' END,
    128 + (n * 16),
    randomblob(128 + (n % 128)),
    lower(hex(randomblob(16))),
    datetime('now', '-' || n || ' hours')
FROM seq;
