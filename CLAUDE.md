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

## UI Design Rules

### Footer Hint Ordering

All input modes must follow this ordering:

```
Actions → Navigation → Help → Close/Cancel → Quit
```

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

- Project config: `.dbx.toml` (in project root)
- Cache directory: `~/.cache/sabiql/<project>/`

## Testing

Visual regression tests verify UI rendering hasn't changed unexpectedly.
See [`tests/README.md`](tests/README.md) for policy and commands.

## Release

1. Update `version` in `Cargo.toml`
2. Commit and push to main
3. Create and push tag: `git tag v1.0.0 && git push origin v1.0.0`
4. GitHub Actions builds and publishes binaries to Releases
