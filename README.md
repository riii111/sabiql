# sabiql
![hero](https://github.com/user-attachments/assets/79b6a108-be29-40ab-89d1-f9dec7a28e8d)

A fast, driver-less TUI to browse, query, and edit PostgreSQL databases — no drivers, no setup, just `psql`.

[![CI](https://github.com/riii111/sabiql/actions/workflows/ci.yml/badge.svg)](https://github.com/riii111/sabiql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Concept

> Vim-first · Safe by design · Oil-and-vinegar UI · Fast and lightweight

sabiql wraps your existing `psql` CLI. No Rust database drivers, no connection pools, no extra dependencies. Point it at your database and get a full-featured TUI. Your `psql` config, `.pgpass`, SSL setup all just work.

Inspired by [oil.nvim](https://github.com/stevearc/oil.nvim)'s "oil and vinegar" philosophy: UI elements appear only when needed, never occupying your screen permanently. Vim-native keybindings (`j/k`, `dd`, `/`) let you navigate and edit without leaving your muscle memory.

Destructive operations are guarded. Inline edits and row deletions always show a preview modal before touching your data. Read-only mode (`Ctrl+R`) goes further — block all writes at the PostgreSQL session level with a single keystroke.

Built in Rust for minimal memory footprint and near-zero idle CPU. A full-featured alternative to GUI tools like DBeaver or DataGrip, without ever leaving the terminal.

## Features
![hero-800](https://github.com/user-attachments/assets/b6b853a0-d7b4-486d-955c-aee74e7a2671)


### Core

- **Read-Only Mode** (`Ctrl+R`) — Toggle safe-browse mode; writes are blocked at both app and DB session level
- **SQL Modal** (`s`) — Ad-hoc queries with auto-completion for tables, columns, and keywords; browse past results with `Ctrl+H`; recall previous queries with `Ctrl+O`
- **ER Diagram** (`e`) — Generate relationship diagrams via Graphviz, opened instantly in your browser
- **Inspector Pane** (`2`) — Column details, types, constraints, and indexes for any table

### Editing

- **Inline Cell Editing** (`e` in Result) — Edit cells in-place with a guarded UPDATE preview before committing
- **Row Deletion** (`dd` in Result) — DELETE with mandatory preview; risk level color-coded (yellow/orange/red)
- **Yank** (`y`) — Copy any cell value to clipboard
- **CSV Export** (`Ctrl+E`) — Export query results to a CSV file

### Navigation

- **Fuzzy Search** (`/`) — Incremental table filtering
- **Focus Mode** (`f`) — Expand any pane to full screen
- **Command Palette** (`Ctrl+K`) — Searchable command list

## Installation

```bash
# macOS / Linux
brew install riii111/sabiql/sabiql

# Cargo (crates.io)
cargo install sabiql

# Arch Linux (AUR)
paru -S sabiql  # or yay -S sabiql

# Void Linux (Unofficial Repo)
echo repository=https://raw.githubusercontent.com/Event-Horizon-VL/blackhole-vl/repository-x86_64 | sudo tee /etc/xbps.d/20-repository-extra.conf
sudo xbps-install -S sabiql

# FreeBSD (ports)
cd /usr/ports/databases/sabiql/ && make install clean

# Install script
curl -fsSL https://raw.githubusercontent.com/riii111/sabiql/main/install.sh | sh
```

## Quick Start

```bash
sabiql
```

On first run, enter your connection details — saved to `~/.config/sabiql/connections.toml`. Press `?` for help.

## Requirements

- `psql` CLI (PostgreSQL client)
- Graphviz (optional, for ER diagrams): `brew install graphviz`

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SABIQL_BROWSER` | Browser for ER diagrams (e.g., `Arc`, `Firefox`). macOS uses `open -a`; falls back to OS default. |

## Roadmap

- [x] Connection management UI
- [x] ER diagram generation
- [x] Read-only mode (`Ctrl+R`)
- [x] SQL modal with DML/DDL safety guardrails
- [x] Query history persistence & fuzzy search
- [x] CSV export & clipboard yank
- [ ] JSON/JSONB support (tree view, editing, validation)
- [ ] Neovim integration (`sabiql.nvim`)
- [ ] Zero-config connection (env vars, `.pgpass`, URI auto-detect)
- [ ] EXPLAIN workflow (plan tree view & comparison)
- [ ] Google Cloud SQL / AlloyDB support
- [ ] MySQL support

Have a feature request? [Open an issue](https://github.com/riii111/sabiql/issues/new) feedback is welcome!

## License

MIT — see [LICENSE](LICENSE).
