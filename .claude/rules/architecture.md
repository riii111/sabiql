---
paths:
  - "**/src/**/*.rs"
---

# Architecture Rules

## Layer Structure (Hexagonal / Ports & Adapters)

```
src/
├── ui/          # Presentation Layer + UI Adapters
├── app/         # Application Layer (State, Reducers, Ports)
├── infra/       # Infrastructure Adapters
└── domain/      # Domain Models (pure data structures)
```

## Dependency Rules

**Allowed:**
- `ui/` → `app/` → `domain/`
- `infra/adapters/` → `app/ports/` (implements traits)
- `ui/adapters/` → `app/ports/` (implements traits)

## Forbidden Dependencies (MUST NOT violate)

- `app/` → `ui/` — use Renderer port instead
- `app/` → `infra/` — use ports like MetadataProvider, ConfigWriter
- `ui/` → `infra/`

If you need app→infra communication, you MUST define a port trait in `app/ports/` and implement it in `infra/adapters/`.

## Ports & Adapters Pattern

Ports are **traits defined in `app/ports/`** that abstract external dependencies:

| Port | Purpose | Adapter Location |
|------|---------|------------------|
| `MetadataProvider` | DB metadata fetching | `infra/adapters/` |
| `QueryExecutor` | SQL execution | `infra/adapters/` |
| `ConfigWriter` | Cache dir | `infra/adapters/` |
| `Renderer` | TUI drawing | `ui/adapters/` |

## Where to Put New Code

| If you need to... | Put it in... |
|-------------------|--------------|
| Add UI component | `ui/components/` |
| Add business logic | `app/` (pure functions, no I/O) |
| Add external I/O | Define port in `app/ports/`, impl in `infra/adapters/` or `ui/adapters/` |
| Add database-specific SQL or connection string logic | Define port in `app/ports/`, impl in `infra/adapters/` |
| Add domain model | `domain/` |
| Add pure calculation used by app | `app/` (e.g., `viewport.rs`, `ddl.rs`) |
| Add key-to-action mapping (simple mode) | `app/keybindings/` (add entry with `combos` to appropriate submodule); `keymap::resolve()` handles it automatically |
| Add key-to-action mapping (Normal mode) | `app/keybindings/normal.rs` + add predicate fn in `mod.rs` + wire in `handler.rs` |
| Add DB-specific SQL or dialect logic | `infra/adapters/{postgres,mysql}/` (NEVER in `app/ports/`) |

## Key Translation Flow

```
crossterm::KeyEvent
  → ui/event/key_translator::translate()
  → app::keybindings::KeyCombo
  → app::keymap::resolve(combo, bindings)   (simple modes)
     OR keybindings::is_quit(&combo) etc.   (Normal mode predicates)
  → Action
```

**Responsibilities:**
- `app/keybindings/`: SSOT module — `KeyBinding` (simple modes) and `ModeRow` (mixed modes with unified display+exec). Split by domain: `normal.rs`, `overlays.rs`, `connections.rs`, `editors.rs`, `types.rs`. Mixed modes use `ModeBindings { rows: &[ModeRow] }`, resolved via `.resolve()`.
- `app/keymap.rs`: `resolve(combo, bindings)` for `KeyBinding` slices; `resolve_mode(combo, rows)` for `ModeRow` slices
- `ui/event/key_translator.rs`: UI adapter — converts `crossterm::KeyEvent` → app-layer `KeyCombo`
- `ui/event/handler.rs`: mode dispatch — calls `ModeBindings::resolve()` or predicate fns, applies context logic

## Side-Effect Boundaries (MUST)

- `app/` MUST be I/O-free. No filesystem, network, or process spawning.
- `domain/` MUST be pure data. No methods with side effects.
- Side effects are ONLY allowed in: `infra/adapters/`, `ui/adapters/`, `main.rs`
- Reducers MUST return `Vec<Effect>` for side effects; NEVER execute them inline.

## Derived State Invariants (MUST)

When an `AppState` field is derived from other fields (e.g. `connection_list_items` is derived from `connections` + `service_entries`), **all source fields MUST be private** and mutated only through setters that automatically rebuild the derived field.

| Pattern | Status |
|---------|--------|
| Direct field assignment (`state.foo = x`) for derived groups | **Forbidden** |
| Setter with auto-rebuild (`state.set_foo(x)`) | **Required** |
| Standalone `rebuild_*()` as public API | **Forbidden** (must be private, called internally by setters) |

Existing enforced group:
- **Connection group**: `connections`, `service_entries` → `connection_list_items`
  - Setters: `set_connections`, `set_service_entries`, `set_connections_and_services`, `retain_connections`
  - Getters: `connections()`, `service_entries()`, `connection_list_items()`

When adding a new derived field to `AppState`, apply the same pattern: private fields + setter with auto-rebuild + read-only getters.

## Key Principles

1. **app/ is I/O-free**: Reducers and state logic have no side effects. Effects are returned as data.
2. **Ports invert dependencies**: app defines what it needs, adapters provide implementations.
3. **UI adapters for UI concerns**: Rendering abstractions live in `ui/adapters/`, not `infra/`.
4. **Domain is pure data**: No business logic in domain models, just structure.

## Postgres Adapter Internal Structure

```
src/infra/adapters/postgres/
├── mod.rs              # struct PostgresAdapter + MetadataProvider + QueryExecutor
│                       # (orchestration: sql/ generates SQL → psql/ executes & parses)
├── psql/               # psql process interaction
│   ├── mod.rs          #   re-exports
│   ├── executor.rs     #   process spawning (I/O, side effects)
│   └── parser.rs       #   stdout → domain types (pure functions)
├── sql/                # SQL string generation (all pure functions)
│   ├── mod.rs          #   re-exports
│   ├── query.rs        #   metadata queries + preview
│   ├── ddl.rs          #   DDL generation (CREATE TABLE)
│   └── dialect.rs      #   DML generation (UPDATE/DELETE)
├── select_guard.rs     # SELECT safety check (pure function)
└── dsn.rs              # DSN construction
```

**Data flow:** `mod.rs` orchestrates → `sql/` generates SQL → `psql/executor.rs` runs psql → `psql/parser.rs` parses output.

**Visibility:** Functions default to private. Use `pub(in crate::infra::adapters::postgres)` for cross-submodule access. Tests use `#[cfg(test)]` within each submodule.

**Quote functions:** Use `crate::infra::utils::{quote_ident, quote_literal}`. Do NOT duplicate as `pg_quote_*`.

## Rendering Strategy

Ratatui requires explicit render control. This app uses **event-driven rendering** (not fixed FPS):

| Trigger | When to render |
|---------|----------------|
| State change | Reducer sets `render_dirty = true`; main loop adds `Effect::Render` |
| Animation deadline | Spinner (150ms), cursor blink (500ms), message timeout, result highlight |
| No activity | Sleep indefinitely until input or deadline |

**Architecture split:**
- `app/render_schedule.rs`: Pure function calculates next deadline (no I/O)
- `main.rs`: UI layer handles `tokio::select!` with `sleep_until(deadline)`
