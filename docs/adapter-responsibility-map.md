# Adapter responsibility map

Database adapters keep app-facing ports stable and split backend-local details by
responsibility.

## Common shape

- `mod.rs`: port implementations and orchestration
- `sql`: backend SQL generation
- CLI executor: external client process execution and session options
- parser: external client output and derived execution metadata

## PostgreSQL

- `mod.rs`: implements metadata and query ports by orchestrating SQL generation and psql execution
- `sql/`: PostgreSQL-specific DDL, query, and write SQL generation
- `psql/executor.rs`: psql invocation, result segmentation, write/export execution
- `psql/parser/`: psql output parsing, SQL splitting, command tag resolution, metadata JSON mapping

## SQLite

- `mod.rs`: implements metadata and query ports, plus SQLite metadata orchestration
- `sql.rs`: SQLite-specific DDL, query, preview, and write SQL generation
- `sqlite3/executor.rs`: sqlite3 invocation, session options, JSON/CSV/quote modes, CSV export
- `sqlite3/parser/lexer.rs`: statement splitting, keyword scanning, write/export classification, execution probes
- `sqlite3/parser/output.rs`: quoted output decoding, typed value recovery, probe stripping, CSV scalar parsing
- `sqlite3/parser/command_tag.rs`: DML/DDL/transaction tag derivation and rollback/savepoint correction

SQLite metadata remains in `mod.rs` because it still mixes async PRAGMA execution
with domain model assembly. Split it only when that flow can move without changing
behavior.
