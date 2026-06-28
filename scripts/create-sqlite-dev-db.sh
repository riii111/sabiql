#!/usr/bin/env sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
db_path="${1:-${SABIQL_SQLITE_DB:-/tmp/sabiql-dev.sqlite3}}"

mkdir -p "$(dirname -- "$db_path")"
rm -f "$db_path"

sqlite3 "$db_path" < "$repo_root/scripts/init-sqlite.sql"
sqlite3 "$db_path" < "$repo_root/scripts/seed-sqlite.sql"

foreign_key_errors=$(sqlite3 "$db_path" "PRAGMA foreign_key_check;")
if [ -n "$foreign_key_errors" ]; then
    printf '%s\n' "$foreign_key_errors" >&2
    exit 1
fi

fts_sync_errors=$(sqlite3 "$db_path" "
BEGIN;

UPDATE agent_messages SET content = content || ' ftssynccheck' WHERE id = 1;
SELECT 'agent_messages_fts stale' WHERE NOT EXISTS (
    SELECT 1 FROM agent_messages_fts WHERE rowid = 1 AND agent_messages_fts MATCH 'ftssynccheck'
);

UPDATE agent_memory_items SET body = body || ' ftssynccheck' WHERE id = 1;
SELECT 'agent_memory_fts stale' WHERE NOT EXISTS (
    SELECT 1 FROM agent_memory_fts WHERE rowid = 1 AND agent_memory_fts MATCH 'ftssynccheck'
);

UPDATE document_chunks SET body = body || ' ftssynccheck' WHERE id = 1;
SELECT 'document_chunks_fts stale' WHERE NOT EXISTS (
    SELECT 1 FROM document_chunks_fts WHERE rowid = 1 AND document_chunks_fts MATCH 'ftssynccheck'
);

ROLLBACK;
")
if [ -n "$fts_sync_errors" ]; then
    printf '%s\n' "$fts_sync_errors" >&2
    exit 1
fi

blob_size_errors=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM file_blobs WHERE byte_size <> length(content);")
if [ "$blob_size_errors" -ne 0 ]; then
    printf 'file_blobs.byte_size mismatch: %s\n' "$blob_size_errors" >&2
    exit 1
fi

table_count=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%';")
printf 'sqlite://%s|%s\n' "$db_path" "$table_count"
