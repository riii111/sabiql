---
paths:
  - "**/src/**/*.rs"
  - "**/tests/**/*.rs"
---

# Testing Obligations

## MUST-test Targets by Layer

| Layer | MUST-test Scenario | Example |
|-------|-------------------|---------|
| **Domain** | Every public constructor / validation | `ConnectionConfig::new()` boundary values |
| **App (reducers)** | Every state transition that changes `AppState` | Action dispatch → state diff |
| **App (ports)** | Default-impl methods on port traits | `DdlGenerator::ddl_line_count()` |
| **Infra (parsers)** | CLI output parsing for each adapter | `psql` tabular output → `QueryResult` |
| **Infra (adapters)** | SQL generation (dialect-specific) | `build_update_sql` for PG (and MySQL when implemented) |
| **UI (components)** | Rendering boundary conditions | Empty table, overflow, error states |
| **Integration** | Cross-layer round-trips | `tests/render_snapshots.rs` |

## `#[ignore]` Tracking Rule (MUST)

- Every `#[ignore]` test MUST have a comment linking to a tracking Issue
- Format: `#[ignore] // tracked: #<issue-number> — <reason>`
- Bare `#[ignore]` without tracking comment is **FORBIDDEN**
- When resolving the linked Issue, the `#[ignore]` MUST be removed or updated

```rust
#[ignore] // tracked: #42 — waiting for MySQL adapter
#[test]
fn mysql_query_parsing() { ... }
```

## Snapshot Test Obligation

- Each new `InputMode` variant MUST have at least one snapshot in `tests/render_snapshots.rs`
- See `visual-regression.md` for detailed coverage criteria

## PR Self-check (for Claude)

Before marking a PR ready:
- [ ] New public functions have unit tests
- [ ] Adapter SQL generation tested for all supported dialects
- [ ] No bare `#[ignore]` without tracking Issue
- [ ] New `InputMode` variants have snapshot tests
