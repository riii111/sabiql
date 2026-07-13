# sabiql
![hero](https://github.com/user-attachments/assets/745ab18f-915c-4017-81a6-465c5c5ee11c)

A fast, driver-less TUI to browse, query, and edit PostgreSQL and SQLite databases — no drivers, no setup, just your database CLI (`psql` or `sqlite3`).

[![CI](https://github.com/riii111/sabiql/actions/workflows/ci.yml/badge.svg)](https://github.com/riii111/sabiql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Concept

> Vim-first · Safe by design · Oil-and-vinegar UI · Fast and lightweight

sabiql wraps your existing database CLI. For PostgreSQL it uses `psql`; for SQLite it uses `sqlite3`. No Rust database drivers, no connection pools, no extra dependencies. Point it at your database and get a full-featured TUI. Your `psql` config, `.pgpass`, and SSL setup all just work for PostgreSQL connections.

Inspired by [oil.nvim](https://github.com/stevearc/oil.nvim)'s "oil and vinegar" philosophy: UI elements appear only when needed, never occupying your screen permanently. Vim-native keybindings (`j/k`, `dd`, `/`) let you navigate and edit without leaving your muscle memory.

Destructive operations are guarded. Inline edits and row deletions always show a preview modal before touching your data. Read-only mode (`Ctrl+R`) goes further — block all writes at the database client level with a single keystroke.

Built in Rust for minimal memory footprint and near-zero idle CPU. A full-featured alternative to GUI tools like DBeaver or DataGrip, without ever leaving the terminal.

## Query Safety

sabiql treats the SQL modal as SQL-only input. CLI meta-commands such as psql backslash commands and sqlite3 dot commands are rejected instead of being passed to the underlying client.

Read-only mode combines app-level write blocking with the database client guard available for the active adapter. PostgreSQL uses a read-only session option. SQLite also runs every sqlite3 command in safe mode, preventing SQL from accessing files, extensions, or databases outside the selected database file.

PostgreSQL multi-statement SQL runs in one transaction. SQLite wraps transactional writes, including persistent PRAGMAs such as `user_version`, unless the input contains transaction control or a session-side-effect / transaction-incompatible statement such as `PRAGMA journal_mode`, `PRAGMA foreign_keys`, or `PRAGMA synchronous`. SQLite safe mode also rejects operations such as `ATTACH` and `VACUUM` that require capabilities unavailable in safe mode.

## Features
![hero_1000_20fps](https://github.com/user-attachments/assets/06e1900d-b044-4f29-a2a8-7d7bab5bd3a1)

### Core

- **Read-Only Mode** (`Ctrl+R`) — Toggle safe-browse mode; writes are blocked at both app and DB session level
- **SQL Modal** (`s`) — Ad-hoc queries with auto-completion for tables, columns, and keywords; recall previous queries with `Ctrl+O`
- **ER Diagram** (`e`) — Generate relationship diagrams via Graphviz, opened instantly in your browser (PostgreSQL only)
- **Inspector Pane** (`2`) — Column details, types, constraints, and indexes for any table

### Editing

- **Inline Cell Editing** (`i` in Result) — Edit cells in-place with a guarded UPDATE preview before committing
- **Row Deletion** (`dd` in Result) — DELETE with mandatory preview; risk level color-coded (yellow/orange/red)
- **Yank** (`y`) — Copy any cell value to clipboard
- **CSV Export** (`Ctrl+E`) — Export query results to a CSV file

### Query Analysis

- **EXPLAIN / EXPLAIN ANALYZE** — PostgreSQL: run your query, then switch tabs to view its execution plan or compare two plans side-by-side.
- **EXPLAIN QUERY PLAN** — SQLite: view query plans for single SELECT statements in the Plan tab.

### Navigation

- **Fuzzy Search** (`/`) — Incremental table filtering
- **Focus Mode** (`f`) — Expand any pane to full screen
- **Settings** (`,`) — Theme, keymap, and ER diagram preferences
- **Command Palette** (`F1`, `:palette`) — Searchable command list

## Installation

```bash
# macOS / Linux
brew install riii111/sabiql/sabiql

# Cargo (crates.io)
cargo install sabiql

# Nix
nix profile install github:riii111/sabiql

# Run once with Nix, without installing
nix run github:riii111/sabiql

# Arch Linux (AUR)
paru -S sabiql  # or yay -S sabiql

# Void Linux (Unofficial Repo)
echo "repository=https://mirror.black-hole.dev/$(xbps-uhelper arch)/" | sudo tee /etc/xbps.d/20-repository-extra.conf
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

For SQLite, you can also pass a database file path or `sqlite://` DSN directly:

```bash
sabiql /path/to/app.db
sabiql /path/to/History
sabiql sqlite:///path/to/app.db
```

On first run without a startup argument, enter your connection details. They are saved to your platform config directory:

- macOS: `~/Library/Application Support/sabiql/connections.toml`
- Linux: `~/.config/sabiql/connections.toml`

For PostgreSQL, fill in host, port, database, and credentials. For SQLite, set **Type** to `SQLite` and enter the path to a database file (for example `/path/to/app.db`).

Press `?` for help.

Open Settings with `,` to switch themes, keymap presets, and the ER diagram browser command.

> **Note:** If you use sabiql inside an IDE terminal, some default keybindings may conflict with the IDE. Open Settings with `,` and switch the keymap preset to make sabiql work comfortably inside your IDE.

## Requirements

Install the CLI for the database you want to open:

- **PostgreSQL:** `psql` (PostgreSQL client)
- **SQLite:** `sqlite3` (SQLite shell), version 3.41.1 or later.

Optional:

- Graphviz (for ER diagrams on PostgreSQL): `brew install graphviz`

### Android / Termux

Android/Termux support is build-only, not full platform support. `cargo install sabiql` should compile on Android, but clipboard yank is unavailable because the desktop clipboard backend is not supported there. Install `psql` for PostgreSQL and `sqlite3` for SQLite.

## SQLite Limitations

SQLite support covers browsing, editing, and ad-hoc SQL on regular database files. Compared with PostgreSQL:

- **File paths only** — Use a regular SQLite database file path or a `sqlite://` DSN to that file. The `sqlite://` form treats everything after the prefix as a raw path; it does not percent-decode URI escapes. In-memory databases (`:memory:`) and SQLite URI filenames (`file:...`) are not supported because sabiql starts `sqlite3` per operation; use a temporary database file instead.
- **No new database files** — Opening a path that does not exist does not create a database.
- **Main database only** — Attached and temporary databases are not browsed as separate namespaces.
- **Grid editing requires a declared primary key** — Regular tables with a declared `PRIMARY KEY` support grid editing. Tables without a declared primary key, views, and virtual tables remain browsable but are read-only targets in the grid.
- **Query plans** — SQLite shows `EXPLAIN QUERY PLAN` in the Plan tab. Plan comparison and `EXPLAIN ANALYZE` are PostgreSQL-only.
- **No ER diagrams** — Graphviz export requires PostgreSQL metadata.
- **No JSON tree view** — Structured JSON editing is PostgreSQL-only.

## Development

With Nix:

```bash
direnv allow
cargo nextest run --workspace
nix build
```

Without direnv, enter the shell explicitly:

```bash
nix develop
```

## Roadmap

- [x] Connection management UI
- [x] ER diagram generation
- [x] Read-only mode (`Ctrl+R`)
- [x] SQL modal with DML/DDL safety guardrails
- [x] Query history persistence & fuzzy search
- [x] CSV export & clipboard yank
- [x] EXPLAIN workflow (plan tree view & comparison)
- [x] JSON/JSONB support (tree view, editing, validation)
- [x] Theme switching (Sabiql Dark / Light)
- [x] SQLite support
- [ ] Neovim integration (`sabiql.nvim`)
- [ ] Zero-config connection (env vars, `.pgpass`, URI auto-detect)
- [ ] Google Cloud SQL / AlloyDB support
- [ ] MySQL support

Have a feature request? [Open an issue](https://github.com/riii111/sabiql/issues/new) feedback is welcome!

## License

MIT — see [LICENSE](LICENSE).
