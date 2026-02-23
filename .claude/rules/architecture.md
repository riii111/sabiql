---
description: Hexagonal architecture rules for sabiql - layer structure, dependency rules, ports & adapters
alwaysApply: false
globs:
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

## Key Principles

1. **app/ is I/O-free**: Reducers and state logic have no side effects. Effects are returned as data.
2. **Ports invert dependencies**: app defines what it needs, adapters provide implementations.
3. **UI adapters for UI concerns**: Rendering abstractions live in `ui/adapters/`, not `infra/`.
4. **Domain is pure data**: No business logic in domain models, just structure.

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
