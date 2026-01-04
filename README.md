# sabiql
![demo-1200](https://github.com/user-attachments/assets/73ad57bd-ad9a-486d-9e27-5b7098563f00)

A fast, driver-less TUI for browsing PostgreSQL databases.

[![CI](https://github.com/riii111/sabiql/actions/workflows/ci.yml/badge.svg)](https://github.com/riii111/sabiql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Concept

sabiql wraps your existing database CLI (psql) — no drivers to install, no connection pools to configure. Just point it at your database and browse.

- **Driver-less**: Uses psql directly, no Rust database drivers needed
- **ER Diagram**: Visualize table relationships via Graphviz export
- **Lightweight**: No persistent connections, event-driven rendering

## Installation

### Using the install script

```bash
curl -fsSL https://raw.githubusercontent.com/riii111/sabiql/main/install.sh | sh
```

### From source

```bash
cargo install --git https://github.com/riii111/sabiql
```

## Quick Start

1. Create `.dbx.toml` in your project root:

```toml
[profiles.default]
dsn = "postgres://user:password@localhost:5432/database"
```

2. Run sabiql:

```bash
sabiql
```

3. Press `?` for help.

## Features

- **Browse Mode**: Navigate schemas and tables with vim-like keybindings
- **SQL Modal**: Execute ad-hoc queries with auto-completion
  ![sql-modal-1200](https://github.com/user-attachments/assets/7505e3b8-bd26-4de9-8cda-a59e6fdbe936)
  Type a few characters and get instant suggestions for tables, columns, and keywords — no manual schema lookup needed.

- **ER Diagram**: Generate relationship diagrams via Graphviz
  ![er-diagram-1400](https://github.com/user-attachments/assets/53d09bea-5013-4b0b-b20c-a861a3d90e1f)

- **Console Integration**: Seamless integration with pgcli

## Keybindings

| Key | Action |
|-----|--------|
| `1`/`2`/`3` | Switch pane (Explorer/Inspector/Result) |
| `j`/`k` | Scroll down/up |
| `g`/`G` | Jump to top/bottom |
| `f` | Toggle focus mode |
| `s` | Open SQL modal |
| `e` | Open ER diagram |
| `c` | Open pgcli console |
| `Ctrl+K` | Command palette |
| `?` | Show help |
| `q` | Quit |

## Requirements

- PostgreSQL (`psql` CLI must be available)
- Graphviz (optional, for ER diagrams): `brew install graphviz`

## Roadmap

- [ ] **Connection UI** — Interactive database connection setup
- [ ] **Expanded viewport** — Wider display area with improved horizontal scrolling
- [ ] **Focused ER diagrams** — Generate diagrams centered on a specific table
- [ ] **MySQL support** — Extend driver-less architecture to MySQL

## License

MIT License - see [LICENSE](LICENSE) for details.
