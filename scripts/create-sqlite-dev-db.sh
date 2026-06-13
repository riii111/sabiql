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

table_count=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%';")
printf 'sqlite://%s|%s\n' "$db_path" "$table_count"
