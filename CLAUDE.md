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
infra/ ─────────────────────────┘
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
│   ├── clipboard/
│   │   ├── mod.rs
│   │   └── pbcopy.rs           # macOS clipboard
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
```

## Key Technical Decisions

### 1. Event Loop Pattern (Ratatui + Tokio)

Use `tokio::select!` for multiplexing input, tick, and render events:

```rust
tokio::select! {
    maybe_event = crossterm_event => { /* handle input */ },
    _ = tick_delay => { event_tx.send(Event::Tick) },
    _ = render_delay => { event_tx.send(Event::Render) },
}
```

Key components:
- `Tui` struct combining Terminal + event handling
- `CancellationToken` for safe task shutdown
- `mpsc::channel` for event communication
- Independent tick/render rates for CPU efficiency

**Reference**: https://ratatui.rs/tutorials/counter-async-app/full-async-events/

### 2. External CLI Execution (Console Mode)

Pattern for spawning pgcli/mycli and returning to TUI:

```rust
fn run_external_cli(terminal: &mut Terminal) -> Result<()> {
    // 1. Leave TUI
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;

    // 2. Execute external CLI
    Command::new("pgcli").arg("-d").arg(&dsn).status()?;

    // 3. Return to TUI
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;
    Ok(())
}
```

**Important**: Pause event handler tasks before spawning external processes to avoid ANSI escape code issues.

**Reference**: https://ratatui.rs/recipes/apps/spawn-vim/

### 3. Async Cache Updates

Use background tasks with Action channels to avoid blocking UI:

```rust
tokio::spawn(async move {
    let metadata = fetch_metadata(&dsn).await?;
    action_tx.send(Action::MetadataLoaded(metadata)).await?;
});
```

### 4. Driver-less Architecture

All DB operations use CLI tools instead of Rust drivers:

| Operation | Method |
|-----------|--------|
| Console | `exec pgcli/mycli` |
| Metadata fetch | `psql -c "SELECT ..." -t -A` with JSON output |
| Preview data | `psql -c "SELECT * LIMIT N"` |

This minimizes dependencies and leverages users' existing DB tools.

### 5. ER Diagram Export (Graphviz)

The ER diagram is generated as DOT from cached table details and exported
to SVG via Graphviz. The SVG is opened in the system viewer.

- Keybinding: `e`
- Command: `:erd`
- Requires Graphviz (`brew install graphviz` on macOS)

## Build Commands

```bash
mise run build              # Build
mise run check              # Type check
mise run clippy             # Lint
mise run fmt                # Format
mise run test               # Run tests
```

## Configuration

- Project config: `.dbx.toml` (in project root)
- Cache directory: `~/.cache/dbtui/<project>/`

## PR Plan

| PR | Scope |
|----|-------|
| PR1 | Scaffold + config + app skeleton |
| PR2 | Core UI: overlays + key UX |
| PR3 | Data layer: metadata + TTL cache + adapters |
| PR4 | Browse polish + SQL Modal + Result history + Copy |
| PR5 | Console integration |
| PR6 | ER diagram export (single-tab) + docs |

## References

- [Ratatui Async Tutorial](https://ratatui.rs/tutorials/counter-async-app/)
- [Ratatui Templates](https://github.com/ratatui-org/templates)
- [Spawn External Editor Recipe](https://ratatui.rs/recipes/apps/spawn-vim/)
