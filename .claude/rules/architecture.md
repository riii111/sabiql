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

**Forbidden:**
- `app/` → `ui/` (use Renderer port instead)
- `app/` → `infra/` (use ports like MetadataProvider, ConfigWriter)
- `ui/` → `infra/`

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
| Add domain model | `domain/` |
| Add pure calculation used by app | `app/` (e.g., `viewport.rs`, `ddl.rs`) |

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
