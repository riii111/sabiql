# dbtui-rs

A Rust-based TUI (Terminal User Interface) application for database browsing and CLI wrapper functionality.

## Features

- **Browse Mode**: Navigate database schemas, tables, and view table details
- **ER Mode**: Visualize table relationships in a neighborhood graph
- **SQL Modal**: Execute ad-hoc SQL queries
- **Console Integration**: Seamless integration with pgcli

## ER Tab

The ER (Entity-Relationship) tab shows foreign key relationships between tables as a neighborhood graph.

### How to Use

1. Switch to the ER tab using `Tab` or `Shift+Tab`
2. The graph shows tables within 1-2 hops via FK relationships
3. Navigate and explore relationships using the keybindings below

### ER Mode Keybindings

| Key | Action |
|-----|--------|
| `j`/`k` | Navigate up/down in node list |
| `Enter` | Recenter graph on selected node |
| `d` | Toggle depth (1 ↔ 2 hops) |
| `1` | Focus Graph pane |
| `2` | Focus Details pane |
| `Esc` | Return to Browse tab |

### Export Commands

| Command | Description |
|---------|-------------|
| `:erd` | Export current graph to DOT file in cache directory |
| `:erd!` | Export to DOT, convert to SVG using Graphviz, and open in viewer |

**Note**: `:erd!` requires Graphviz to be installed (`brew install graphviz` on macOS)

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
