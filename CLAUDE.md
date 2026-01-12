# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**sabiql** is a fast, driver-less TUI for browsing PostgreSQL databases.

- **Tech Stack**: Rust + Ratatui + Tokio
- **Target DB**: PostgreSQL (MVP), MySQL (future)
- **No DB Driver Required**: Uses `psql`/`mysql` CLI for queries (driver-less architecture)

## Architecture

### Layer Structure (Hexagonal / Ports & Adapters)

```
src/
├── ui/          # Presentation Layer + UI Adapters
├── app/         # Application Layer (State, Reducers, Ports)
├── infra/       # Infrastructure Adapters
└── domain/      # Domain Models (pure data structures)
```

### Dependency Rules

**Allowed:**
- `ui/` → `app/` → `domain/`
- `infra/adapters/` → `app/ports/` (implements traits)
- `ui/adapters/` → `app/ports/` (implements traits)

**Forbidden:**
- `app/` → `ui/` (use Renderer port instead)
- `app/` → `infra/` (use ports like MetadataProvider, ConfigWriter)
- `ui/` → `infra/`

### Ports & Adapters Pattern

Ports are **traits defined in `app/ports/`** that abstract external dependencies:

| Port | Purpose | Adapter Location |
|------|---------|------------------|
| `MetadataProvider` | DB metadata fetching | `infra/adapters/` |
| `QueryExecutor` | SQL execution | `infra/adapters/` |
| `ConfigWriter` | Cache dir | `infra/adapters/` |
| `Renderer` | TUI drawing | `ui/adapters/` |

### Where to Put New Code

| If you need to... | Put it in... |
|-------------------|--------------|
| Add UI component | `ui/components/` |
| Add business logic | `app/` (pure functions, no I/O) |
| Add external I/O | Define port in `app/ports/`, impl in `infra/adapters/` or `ui/adapters/` |
| Add domain model | `domain/` |
| Add pure calculation used by app | `app/` (e.g., `viewport.rs`, `ddl.rs`) |

### Key Principles

1. **app/ is I/O-free**: Reducers and state logic have no side effects. Effects are returned as data.
2. **Ports invert dependencies**: app defines what it needs, adapters provide implementations.
3. **UI adapters for UI concerns**: Rendering abstractions live in `ui/adapters/`, not `infra/`.
4. **Domain is pure data**: No business logic in domain models, just structure.

### Rendering Strategy

Ratatui requires explicit render control. This app uses **event-driven rendering** (not fixed FPS):

| Trigger | When to render |
|---------|----------------|
| State change | Reducer sets `render_dirty = true`; main loop adds `Effect::Render` |
| Animation deadline | Spinner (150ms), cursor blink (500ms), message timeout, result highlight |
| No activity | Sleep indefinitely until input or deadline |

**Architecture split:**
- `app/render_schedule.rs`: Pure function calculates next deadline (no I/O)
- `main.rs`: UI layer handles `tokio::select!` with `sleep_until(deadline)`

## UI Design Rules

### Component Structure (Atomic Design)

UI components follow the Atomic Design pattern:

```
src/ui/components/
├── atoms/       # Smallest reusable units (spinner, key_chip, panel_border)
├── molecules/   # Compositions of atoms (modal_frame, hint_bar)
└── *.rs         # Organisms: screen-level components (explorer, inspector, etc.)
```

| Layer | Purpose | Examples |
|-------|---------|----------|
| atoms | Single-purpose primitives | `spinner_char()`, `key_chip()`, `panel_block()` |
| molecules | Reusable patterns combining atoms | `render_modal()`, `hint_line()` |
| organisms | Screen sections, may use molecules/atoms | `Explorer`, `SqlModal`, `Footer` |

When adding UI components:
- Extract repeated visual patterns into atoms/molecules
- Use `Theme::*` tokens instead of raw `Color::*` values
- Organisms should compose molecules/atoms, not duplicate their logic

### Footer Hint Ordering

All input modes must follow this ordering:

```
Actions → Navigation → Help → Close/Cancel → Quit
```

### Keybindings & Commands

Keybinding and command definitions follow this architecture:

| Concept | Location | Responsibility |
|---------|----------|----------------|
| Data definitions | `app/keybindings.rs` | Single source of truth for key/description/Action |
| Display logic | `ui/components/footer.rs` | Context-sensitive hint selection by InputMode/state |
| Full reference | `ui/components/help_overlay.rs` | Complete keybinding reference |
| Command list | `app/palette.rs` | Commands shown in Command Palette |

When adding a new keybinding:
1. Add data to `keybindings.rs`
2. Implement event handler in `handler.rs`
3. Update Footer/Help/Palette as needed

## Build Commands

```bash
mise run build              # Build
mise run clippy             # Lint
mise run fmt                # Format
mise run test               # Run tests
```

### Snapshot Testing

```bash
mise exec -- cargo insta review    # Review pending snapshots
mise exec -- cargo insta accept    # Accept all pending snapshots
```

## Configuration

- Connection config: `~/.config/sabiql/connections.toml`
- Cache directory: `~/.cache/sabiql/<project>/`

## Testing

Visual regression tests verify UI rendering hasn't changed unexpectedly.
See [`tests/README.md`](tests/README.md) for policy and commands.

## Release

1. Update `version` in `Cargo.toml`
2. Commit and push to main
3. Create and push tag: `git tag v1.0.0 && git push origin v1.0.0`
4. GitHub Actions builds and publishes binaries to Releases
