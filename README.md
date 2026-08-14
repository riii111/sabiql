# sabiql
![hero](https://github.com/user-attachments/assets/745ab18f-915c-4017-81a6-465c5c5ee11c)

A fast, driver-less TUI for browsing, querying, and editing PostgreSQL and SQLite databases from the terminal. It works with the database CLI you already use: `psql` or `sqlite3`.

[![CI](https://github.com/riii111/sabiql/actions/workflows/ci.yml/badge.svg)](https://github.com/riii111/sabiql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Why sabiql?

> Vim-first · Safe by design · Inspired by oil.nvim · Fast and lightweight

sabiql brings database browsing, querying, and editing into the terminal without replacing your existing database setup. PostgreSQL connections continue to use your `psql` configuration, `.pgpass`, and SSL settings.

The interface stays out of your way. Inspired by [oil.nvim](https://github.com/stevearc/oil.nvim)'s "oil and vinegar" philosophy, UI elements appear only when needed. Vim-native keybindings such as `j/k`, `dd`, and `/` keep navigation and editing familiar.

Inline edits and row deletions always show a preview before touching your data. Read-only mode (`Ctrl+R`) also blocks writes at the database client level.

## Features

![hero_1000_20fps](https://github.com/user-attachments/assets/06e1900d-b044-4f29-a2a8-7d7bab5bd3a1)

- **Browse and inspect** — Find tables with fuzzy search and inspect columns, types, constraints, and indexes
- **Run SQL** — Write ad-hoc queries with completion for tables, columns, and keywords, then recall them from query history
- **Edit with previews** — Update cells or delete rows only after reviewing the SQL and its risk level
- **Browse in read-only mode** (`Ctrl+R`) — Block writes while investigating data
- **Analyze queries** — View and compare PostgreSQL execution plans or inspect SQLite query plans
- **Work with data** — Copy cell values, export CSV, and inspect or edit PostgreSQL JSON/JSONB documents
- **Visualize relationships** — Generate PostgreSQL ER diagrams with Graphviz and open them in your browser

Press `?` inside sabiql to see all commands and keybindings.

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

# Windows x86_64 (experimental)
# Download sabiql-x86_64-pc-windows-msvc.zip from GitHub Releases,
# extract sabiql.exe, and add its directory to PATH.

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

Launch sabiql and enter your connection details:

```bash
sabiql
```

You can also open an existing SQLite database directly:

```bash
sabiql /path/to/app.db
```

Use `Ctrl+R` before browsing data when you want to block writes. Press `?` for help, or open Settings with `,` to change the theme and keymap.

## Requirements

Install the CLI for the database you want to open:

- **PostgreSQL:** `psql` (PostgreSQL client)
- **SQLite:** `sqlite3` (SQLite shell), version 3.41.1 or later

Graphviz is optional and enables ER diagrams for PostgreSQL. Windows support is experimental.

SQLite works with existing, regular database files. See [SQLite support and limitations](docs/sqlite.md) for details.

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

Have a feature request? [Open an issue](https://github.com/riii111/sabiql/issues/new). Feedback is welcome!

## License

MIT — see [LICENSE](LICENSE).
