# sabiql
<img width="1400" height="873" alt="hero" src="https://github.com/user-attachments/assets/d94720a1-7b28-4dfe-a5ca-1c9cedf415eb" />

A fast, driver-less TUI for browsing PostgreSQL databases.

[![CI](https://github.com/riii111/sabiql/actions/workflows/ci.yml/badge.svg)](https://github.com/riii111/sabiql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Concept

sabiql wraps your existing database CLI (psql) — no drivers to install, no connection pools to configure. Just point it at your database and browse with vim-like keybindings.

Built to be driver-less and lightweight (requires psql, but no Rust database drivers). No persistent connections, just event-driven rendering when you need it.

## Features

- **SQL Modal**: Execute ad-hoc queries with auto-completion
  ![sql-modal-1200](https://github.com/user-attachments/assets/7505e3b8-bd26-4de9-8cda-a59e6fdbe936)
  Type a few characters and get instant suggestions for tables, columns, and keywords — no manual schema lookup needed.

- **ER Diagram**: Generate relationship diagrams via Graphviz
  Press `e` to instantly open an ER diagram in your browser — see table relationships at a glance.
  ![er-diagram-1400](https://github.com/user-attachments/assets/53d09bea-5013-4b0b-b20c-a861a3d90e1f)

- **Inspector Pane**: View column details, types, constraints, and indexes for any table
  ![inspector-1400](https://github.com/user-attachments/assets/fde64b47-fe1f-417b-9b9f-f65dcdac32c6)

- **Fuzzy Search**: Quickly find tables with incremental filtering
  ![fuzzy-search-1400](https://github.com/user-attachments/assets/4daf8b7a-cf24-4a09-b93a-f7aa9a63cadd)

- **Focus Mode**: Expand any pane to full screen with `f`
  ![focus-mode-1400](https://github.com/user-attachments/assets/7412e206-cc64-4652-9185-2269592e8d65)


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

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SABIQL_BROWSER` | Custom browser/app name for ER diagrams (e.g., `Arc`, `Firefox`). On macOS, uses `open -a` automatically. Falls back to OS default if unset. |

## Roadmap

- [ ] **Connection UI** — Interactive database connection setup
- [ ] **Expanded viewport** — Wider display area with improved horizontal scrolling
- [ ] **Focused ER diagrams** — Generate diagrams centered on a specific table
- [ ] **MySQL support** — Extend driver-less architecture to MySQL

## License

MIT License - see [LICENSE](LICENSE) for details.
