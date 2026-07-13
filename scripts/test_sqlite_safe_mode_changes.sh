#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
classifier="$script_dir/sqlite_safe_mode_changes.sh"

assert_classification() {
	local expected=$1
	shift

	local actual
	actual=$(bash "$classifier" "$@")
	if [ "$actual" != "sqlite_safe_mode=$expected" ]; then
		printf 'expected %s, got %s for: %s\n' "$expected" "$actual" "$*" >&2
		exit 1
	fi
}

while IFS='|' read -r expected paths; do
	if [ "$expected" = expected ]; then
		continue
	fi

	IFS=',' read -r -a changed_files <<<"$paths"
	assert_classification "$expected" "${changed_files[@]}"
done <<'CASES'
expected|paths
true|src/infra/adapters/sqlite/sqlite3/metadata.rs
true|src/infra/adapters/test_support.rs
true|src/app/ports/outbound/db_operation_error.rs
true|src/app/ports/outbound/sqlite_path_validator.rs
true|src/app/model/connection/error.rs
true|src/app/update/connection/error.rs
true|src/domain/connection/sqlite_path.rs
true|src/ui/features/connections/error.rs
true|Cargo.lock
true|.github/workflows/ci.yml
true|README.md,src/infra/adapters/sqlite/sqlite3/metadata.rs
false|README.md
false|src/app/ports/outbound/clipboard.rs
false|src/app/ports/outbound/renderer.rs
false|src/app/ports/outbound/settings_store.rs
false|src/app/update/connection/lifecycle.rs
false|src/domain/connection/config.rs
false|src/infra/adapters/postgres/adapter.rs
CASES
