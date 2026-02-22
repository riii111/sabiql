# sabiql
<img width="1400" height="920" alt="hero" src="https://github.com/user-attachments/assets/de30d808-118c-4847-b838-94e638986822" />

A fast, driver-less TUI to browse, query, and edit PostgreSQL databases — no drivers, no setup, just `psql`.

[![CI](https://github.com/riii111/sabiql/actions/workflows/ci.yml/badge.svg)](https://github.com/riii111/sabiql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Concept

sabiql wraps your existing `psql` CLI — no Rust database drivers, no connection pools, no extra dependencies. Point it at your database and get a full-featured TUI with vim-like keybindings.

Built in Rust for minimal memory footprint and near-zero idle CPU — no runtime, no GC pauses.

## Features

https://github.com/user-attachments/assets/7d2c34ae-94b7-4746-86a5-6aadd0a4ab45

### Core

- **SQL Modal** (`s`) — Ad-hoc queries with auto-completion for tables, columns, and keywords
- **ER Diagram** (`e`) — Generate relationship diagrams via Graphviz, opened instantly in your browser
- **Inspector Pane** (`2`) — Column details, types, constraints, and indexes for any table

### Editing

- **Inline Cell Editing** (`e` in Result) — Edit cells in-place with a guarded UPDATE preview before committing
- **Row Deletion** (`dd` in Result) — DELETE with mandatory preview; risk level color-coded (yellow/orange/red)
- **Yank** (`y`) — Copy any cell value to clipboard

### Navigation

- **Fuzzy Search** (`/`) — Incremental table filtering
- **Focus Mode** (`f`) — Expand any pane to full screen
- **Command Palette** (`Ctrl+K`) — Searchable command list

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/riii111/sabiql/main/install.sh | sh
```

Or from source:

```bash
cargo install --git https://github.com/riii111/sabiql
```

## Quick Start

```bash
sabiql
```

On first run, enter your connection details — saved to `~/.config/sabiql/connections.toml`. Press `?` for help.

## Keybindings

| Key | Action |
|-----|--------|
| `1`/`2`/`3` | Switch pane (Explorer/Inspector/Result) |
| `j`/`k` | Scroll down/up |
| `g`/`G` | Jump to top/bottom |
| `f` | Toggle focus mode |
| `s` | Open SQL modal |
| `e` | Open ER diagram / Edit cell (Result pane) |
| `dd` | Delete row (Result pane, with preview) |
| `y` | Yank cell value |
| `Ctrl+K` | Command palette |
| `?` | Show help |
| `q` | Quit |

## Requirements

- `psql` CLI (PostgreSQL client)
- Graphviz (optional, for ER diagrams): `brew install graphviz`

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SABIQL_BROWSER` | Browser for ER diagrams (e.g., `Arc`, `Firefox`). macOS uses `open -a`; falls back to OS default. |

## Roadmap

- [x] Connection UI
- [x] Focused ER diagrams
- [x] Expanded viewport / horizontal scrolling
- [ ] Google Cloud SQL / AlloyDB support
- [ ] MySQL support

## License

MIT — see [LICENSE](LICENSE).
