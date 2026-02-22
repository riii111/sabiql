# sabiql
<img width="1400" height="920" alt="hero" src="https://github.com/user-attachments/assets/de30d808-118c-4847-b838-94e638986822" />


A fast, driver-less TUI for browsing PostgreSQL databases.

[![CI](https://github.com/riii111/sabiql/actions/workflows/ci.yml/badge.svg)](https://github.com/riii111/sabiql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Concept

sabiql wraps your existing database CLI (psql) — no drivers to install, no connection pools to configure. Just point it at your database and browse with vim-like keybindings.

Built to be driver-less and lightweight (requires psql, but no Rust database drivers). No persistent connections, just event-driven rendering when you need it.

## Features

https://github.com/user-attachments/assets/7d2c34ae-94b7-4746-86a5-6aadd0a4ab45

- **SQL Modal**: Execute ad-hoc queries with auto-completion
  Type a few characters and get instant suggestions for tables, columns, and keywords — no manual schema lookup needed.

- **ER Diagram**: Generate relationship diagrams via Graphviz
  Press `e` to instantly open an ER diagram in your browser — see table relationships at a glance.

- **Inspector Pane**: View column details, types, constraints, and indexes for any table

- **Fuzzy Search**: Quickly find tables with incremental filtering

- **Focus Mode**: Expand any pane to full screen with `f`

- **Inline Cell Editing**: Edit result cells in-place with a guarded UPDATE preview before committing
  Press `e` on any result cell to enter edit mode, then `:w` to preview and confirm the UPDATE.

- **Row Deletion**: Delete rows via `dd` with a mandatory DELETE preview and confirmation
  Risk-aware guardrails color-code the preview (yellow/orange/red) and block dangerous operations automatically.


## Installation

### Using the install script

Downloads the latest release binary and places it in `~/.local/bin`. ([view source](https://github.com/riii111/sabiql/blob/main/install.sh))

```bash
curl -fsSL https://raw.githubusercontent.com/riii111/sabiql/main/install.sh | sh
```

### From source

```bash
cargo install --git https://github.com/riii111/sabiql
```

## Quick Start

1. Run sabiql:

```bash
sabiql
```

2. Enter your connection details in the setup screen (first run only)
   - Connection info is saved to `~/.config/sabiql/connections.toml`

3. Press `?` for help.

## Keybindings

| Key | Action |
|-----|--------|
| `1`/`2`/`3` | Switch pane (Explorer/Inspector/Result) |
| `j`/`k` | Scroll down/up |
| `g`/`G` | Jump to top/bottom |
| `f` | Toggle focus mode |
| `s` | Open SQL modal |
| `e` | Open ER diagram / Edit cell (in Result pane) |
| `dd` | Delete row (in Result pane, with preview) |
| `y` | Yank (copy) cell value |
| `Ctrl+K` | Command palette |
| `?` | Show help |
| `q` | Quit |

## Requirements

- PostgreSQL (`psql` CLI must be available)
- Graphviz (optional, for ER diagrams): `brew install graphviz`

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SABIQL_BROWSER` | Custom browser/app name for ER diagrams (e.g., `Arc`, `Firefox`). On macOS, uses `open -a` automatically. Falls back to OS default if unset. |

## Roadmap

- [x] **Connection UI** — Interactive database connection setup
- [x] **Focused ER diagrams** — Generate diagrams centered on a specific table
- [ ] **Expanded viewport** — Wider display area with improved horizontal scrolling
- [ ] **MySQL support** — Extend driver-less architecture to MySQL

## License

MIT License - see [LICENSE](LICENSE) for details.
