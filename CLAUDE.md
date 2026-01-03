# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**dbtui-rs** is a Rust-based TUI (Terminal User Interface) application that integrates database browsing and CLI wrapper functionality. It combines the features of `dbn` (table browser) and `dbx` (pgcli/mycli launcher) into a unified interface.

- **Tech Stack**: Rust + Ratatui + Tokio
- **Target DB**: PostgreSQL (MVP), MySQL (future)
- **No DB Driver Required**: Uses `psql`/`mysql` CLI for queries (driver-less architecture)

## Architecture

### Three-Layer Structure (Simplified Hexagonal)

```
src/
├── ui/          # Presentation Layer
├── app/         # Application Layer (includes Ports)
├── infra/       # Infrastructure Layer (Adapters)
└── domain/      # Domain Models
```

### Layer Responsibilities

| Layer | Directory | Responsibility |
|-------|-----------|----------------|
| **Presentation** | `ui/` | TUI rendering, event loop, user input handling |
| **Application** | `app/` | State management, actions, port definitions (traits) |
| **Infrastructure** | `infra/` | Adapters (DB CLI wrappers), cache, config parsing |
| **Domain** | `domain/` | Core models (Table, Column, ForeignKey, Index, Schema) |

### Dependency Direction

```
ui/ ──uses──> app/ ──uses──> domain/
                │
                └──defines──> ports/ (traits)
                                 ▲
                                 │ implements
infra/ ──────────────────────────┘
```

### Directory Structure

```
src/
├── main.rs
│
├── ui/                         # ── Presentation Layer ──
│   ├── mod.rs
│   ├── tui.rs                  # Terminal + Event loop (tokio::select!)
│   ├── event/
│   │   ├── mod.rs
│   │   ├── handler.rs          # Event → Action conversion
│   │   └── key.rs              # Key mappings
│   └── components/
│       ├── mod.rs
│       ├── layout.rs
│       ├── header.rs
│       ├── footer.rs
│       ├── explorer.rs
│       ├── inspector.rs
│       ├── result.rs
│       └── status_message.rs
│
├── app/                        # ── Application Layer ──
│   ├── mod.rs
│   ├── state.rs                # AppState (UI state + domain state)
│   ├── mode.rs                 # Browse mode (single-tab)
│   ├── action.rs               # Action enum (state update triggers)
│   └── ports/                  # Port definitions (traits)
│       ├── mod.rs
│       ├── metadata.rs         # MetadataProvider trait
│       ├── template.rs         # TemplateGenerator trait
│       └── clipboard.rs        # ClipboardWriter trait
│
├── infra/                      # ── Infrastructure Layer ──
│   ├── mod.rs
│   ├── adapters/               # Adapter implementations
│   │   ├── mod.rs
│   │   ├── postgres.rs         # PostgresAdapter (uses psql CLI)
│   │   └── mysql.rs            # MysqlAdapter (stub)
│   ├── cache/
│   │   ├── mod.rs
│   │   └── ttl_cache.rs        # TTL-based metadata cache
│   ├── export/
│   │   ├── mod.rs
│   │   └── dot.rs              # Graphviz DOT export
│   └── config/
│       ├── mod.rs
│       ├── dbx_toml.rs         # .dbx.toml parser
│       └── project_root.rs     # Project root detection
│
├── domain/                     # ── Domain Models ──
│   ├── mod.rs
│   ├── schema.rs
│   ├── table.rs
│   ├── column.rs
│   ├── foreign_key.rs
│   └── index.rs
│
└── error.rs                    # Common error types

tests/                          # ── Integration Tests ──
├── harness/
│   ├── mod.rs                  # Test utilities
│   └── fixtures.rs             # Sample data builders
├── render_snapshots.rs         # Visual regression tests
└── snapshots/                  # Generated .snap files
```

## Build Commands

```bash
mise run build              # Build
mise run clippy             # Lint
mise run fmt                # Format
mise run test               # Run tests
```

## Configuration

- Project config: `.dbx.toml` (in project root)
- Cache directory: `~/.cache/dbtui/<project>/`

## Testing

Visual regression tests verify UI rendering hasn't changed unexpectedly.
See [`tests/README.md`](tests/README.md) for policy and commands.
