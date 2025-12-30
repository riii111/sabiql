# dbtui-rs

A Rust-based TUI (Terminal User Interface) application for database browsing and CLI wrapper functionality.

## Features

- **Browse Mode**: Navigate database schemas, tables, and view table details
- **ER Diagram**: Visualize all table relationships via Graphviz export
- **SQL Modal**: Execute ad-hoc SQL queries
- **Console Integration**: Seamless integration with pgcli

## ER Diagram

The ER (Entity-Relationship) diagram visualizes foreign key relationships between all tables in the database.

### How to Use

1. Press `e` in Browse mode or use `:erd` command
2. The diagram shows all tables and FK relationships loaded in the cache
3. The diagram is exported as DOT format and converted to SVG for viewing

**Note**: Requires Graphviz to be installed (`brew install graphviz` on macOS)

## Browse Mode Keybindings

| Key | Action |
|-----|--------|
| `1`/`2`/`3` | Switch pane (Explorer/Inspector/Result) |
| `f` | Toggle focus mode (Result pane fullscreen) |
| `j`/`k` | Scroll down/up |
| `g`/`G` | Scroll to top/bottom |
| `h`/`l` | Scroll left/right |
| `[`/`]` | Previous/Next inspector tab |
| `r` | Reload metadata |
| `c` | Open pgcli console |
| `s` | Open SQL modal |
| `e` | Open ER diagram (via Graphviz) |
| `Ctrl+P` | Open table picker |
| `Ctrl+K` | Open command palette |
| `?` | Show help |
| `q` | Quit |

## Configuration

Create a `.dbx.toml` file in your project root:

```toml
[profiles.default]
dsn = "postgres://user:password@localhost:5432/database"
```
