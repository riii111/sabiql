---
paths:
  - "**/src/app/ports/**/*.rs"
  - "**/src/infra/adapters/**/*.rs"
---

# DB-Agnostic Rules

## Port-level Neutrality (MUST)

- Port traits in `app/ports/` MUST NOT contain PostgreSQL-specific SQL or syntax
- Port method signatures MUST use generic types (no `PgType`, no PG-specific enums)
- Port documentation MUST describe behavior without referencing a specific RDBMS

## Adapter Isolation (MUST)

- All DB-specific SQL, quoting, and connection string logic MUST live in `infra/adapters/{postgres,mysql}/`
- Adapters MUST NOT leak dialect-specific types into port return types
- When adding a feature to one adapter, open a tracking Issue for the other adapter

## Extension Readiness Checklist

When modifying any port trait:
1. Verify the new method signature is dialect-neutral
2. Check if existing PG adapter impl uses PG-specific syntax that should be abstracted
3. If MySQL adapter stub exists, verify it compiles (even if `#[ignore]` tested)

## Adapter Internal Submodule Conventions

Each adapter directory (e.g., `postgres/`) follows this structure:

- **`mod.rs`**: Struct definition + port trait impls (`MetadataProvider`, `QueryExecutor`). Orchestration only — no SQL generation or parsing logic.
- **`psql/` (or `mysql/` CLI dir)**: Process execution (`executor.rs`) and output parsing (`parser.rs`). Side effects are confined to `executor.rs`.
- **`sql/`**: Pure SQL string generation. Split by concern: `query.rs` (metadata), `ddl.rs` (DDL), `dialect.rs` (DML).
- **Utility modules** (`select_guard.rs`, `dsn.rs`): Single-purpose pure functions.

**When adding a MySQL adapter**, mirror this structure: `mysql/mod.rs`, `mysql/mysql_cli/`, `mysql/sql/`, etc. Port trait impls go in `mod.rs`; dialect-specific SQL goes in `sql/`.

## Current Adapter Status

- PostgreSQL: primary, fully implemented
- MySQL: planned, not yet implemented
