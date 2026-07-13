#!/usr/bin/env bash
set -euo pipefail

for changed_file in "$@"; do
	case "$changed_file" in
	.github/workflows/ci.yml | scripts/sqlite_safe_mode_changes.sh | scripts/test_sqlite_safe_mode_changes.sh | Cargo.lock | Cargo.toml | rust-toolchain.toml | src/app/Cargo.toml | src/domain/Cargo.toml | src/infra/Cargo.toml | src/infra/adapters/registry.rs | src/infra/adapters/test_support.rs | src/infra/adapters/sqlite/* | src/app/ports/outbound/db_operation_error.rs | src/app/ports/outbound/sqlite_path_validator.rs | src/app/model/connection/error.rs | src/app/model/connection/error_state.rs | src/app/update/connection/error.rs | src/domain/connection/database_type.rs | src/domain/connection/sqlite_path.rs | src/ui/features/connections/error.rs)
		echo 'sqlite_safe_mode=true'
		exit 0
		;;
	esac
done

echo 'sqlite_safe_mode=false'
