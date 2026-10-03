# sabiql
![hero](https://github.com/user-attachments/assets/745ab18f-915c-4017-81a6-465c5c5ee11c)

Fast, safe-by-design, Vim-first DB TUI with ER diagrams.

[![CI](https://github.com/riii111/sabiql/actions/workflows/ci.yml/badge.svg)](https://github.com/riii111/sabiql/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Concept

> Vim-first · Safe by design · Oil-and-vinegar UI · Fast and lightweight

sabiql brings database browsing, querying, and editing into the terminal while using the native CLI for each database. PostgreSQL connections can reuse existing libpq connection settings such as `.pgpass`, `pg_service.conf`, and SSL settings.

Like [oil.nvim](https://github.com/stevearc/oil.nvim), sabiql keeps its interface out of your way. Following oil.nvim's "oil and vinegar" philosophy, UI elements appear only when needed. Vim-native keybindings such as `j/k`, `dd`, and `/` keep navigation and editing familiar.

Safety follows a plan-before-apply flow familiar from Terraform: inline edits and row deletions show the SQL and its risk level before you confirm the change. Read-only mode (`Ctrl+R`) also blocks writes at the database client level.

## Features

![hero_1000_20fps](https://github.com/user-attachments/assets/06e1900d-b044-4f29-a2a8-7d7bab5bd3a1)

- **Browse and inspect** — Find tables with fuzzy search, inspect columns, constraints, indexes, foreign keys, triggers, and DDL
- **Run SQL** — Write ad-hoc queries with completion for tables, columns, and keywords, then recall them from query history
- **Edit with previews** — Update cells or delete rows only after reviewing the SQL and its risk level
- **Browse in read-only mode** (`Ctrl+R`) — Block writes while investigating data
- **Analyze queries** — View and compare PostgreSQL or MySQL execution plans, or inspect SQLite query plans
- **Work with data** — Copy cell values, export CSV, and inspect or edit PostgreSQL and MySQL JSON documents
- **Visualize relationships** — Generate PostgreSQL and MySQL ER diagrams with Graphviz and open them in your browser

Press `?` inside sabiql to see all commands and keybindings.

## Installation

```bash
# macOS / Linux
brew install sabiql

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

## Database Setup

sabiql uses the CLI for the database you want to open:

- **To use PostgreSQL:** install `psql`
- **To use MySQL:** install the Oracle MySQL `mysql` CLI 8.4.x. Oracle MySQL servers 5.7, 8.0, and 8.4 can be connected to; 8.4 is the continuously validated server version and older or newer Oracle server versions are not fully guaranteed.
- **To use SQLite:** macOS includes `sqlite3` by default. Check that it is version 3.41.1 or later; additional installation is usually unnecessary. On Linux and other platforms, install `sqlite3` version 3.41.1 or later.

Graphviz is required only for PostgreSQL and MySQL ER diagrams. SQLite does not support ER diagrams.

ER diagrams are generated and opened on the host running sabiql. Running sabiql itself on an SSH or other headless host is outside the guaranteed workflow for ER generation and viewer display; a local sabiql instance connecting to a database through an SSH tunnel remains supported.

Windows support is experimental.

See [MySQL support and limitations](docs/mysql.md) and [SQLite support and limitations](docs/sqlite.md) for supported versions and database-specific limitations.

## Quick Start

Launch sabiql and enter your connection details:

```bash
sabiql
```

You can also open an existing SQLite database directly:

```bash
sabiql /path/to/app.db
```

For a non-saved PostgreSQL or MySQL connection, pass a URI or the name of an
environment variable containing one:

```bash
sabiql 'postgresql://user@localhost/app'
sabiql --connection-env DATABASE_URL
```

Supported URI schemes are `postgres://`, `postgresql://`, and `mysql://`.
Passing a URI with credentials can expose the secret in shell history and
process arguments; `--connection-env` avoids putting the URI in the command
line. The connection profile is not saved, but query history and CSV exports
follow their normal persistence behavior.

Use `Ctrl+R` before browsing data when you want to block writes. Press `?` for help, or open Settings with `,` to change the theme and keymap.

Copying defaults to **Auto**: on SSH (`SSH_CONNECTION` or `SSH_TTY` is nonempty), sabiql sends OSC 52 to your terminal; otherwise it tries the OS clipboard and falls back to OSC 52 if that fails. Open Settings with `,`, select **Clipboard**, and choose Auto, OS clipboard, or Terminal (OSC 52). A saved change applies to the next copy, without a restart. Cancel or a failed save leaves the previous choice active. Existing explicit `clipboard_backend = "native"` or `"osc52"` settings remain respected; an omitted value means `"auto"`.

Your terminal and any multiplexer must support and allow OSC 52. The success message distinguishes OS copying from sending to the terminal: sending cannot confirm clipboard acceptance. Values over 74,994 UTF-8 bytes are rejected without truncation. SSH variables may be absent inside containers; use the manual choice when necessary.

### Saved passwords and recovery

New or edited passwords are stored in macOS Keychain, Windows Credential Manager, or Secret Service on Linux/FreeBSD. Linux needs an available Secret Service implementation, such as GNOME Keyring, KWallet, or KeePassXC, and a working login/session bus. FreeBSD source builds additionally require system `dbus` and `pkgconf`; its keyring dependency is not vendored. Android/Termux does not support password storage. Existing plaintext passwords remain readable and are migrated only when that connection is edited and saved; there is no automatic plaintext fallback.

If secure storage is unavailable, PostgreSQL can use an empty saved password with `.pgpass` or a PostgreSQL service. Both PostgreSQL and MySQL can use a URI from `--connection-env`. MySQL does not read the user's usual option/login-path files through this adapter. Renaming a legacy connection with a password also needs secure storage; clear its password first if using an alternative authentication source.

A failed password lookup does not mean your saved connections were deleted. Fix the store's lock, permissions, service or login session, then restart sabiql. Store operations have a 60-second deadline and do not hold the settings-file lock while awaiting an OS prompt. A missing credential keeps its reference when unrelated connection fields are edited; set a new password to restore it. Configuration files do not carry the OS store entries between machines. After a configuration change is written, an old-password cleanup failure does not roll it back: a save keeps its replacement credential, and a deleted connection stays deleted. The warning means an old OS-store credential may remain, or deletion completed without acknowledgement. Verify the affected credential before removing it manually.

Settings retain their existing configuration version, and new passwordless files use version 3. Saving a password reference upgrades the file to version 4, which sabiql v3.0.1 and earlier cannot read. Back up configuration before upgrading if you need to roll back; do not delete the file in response to a version mismatch. Use a compatible sabiql version instead.

PostgreSQL URI password values containing literal `?` or `#` must encode them as `%3F` or `%23`. MySQL URI options use their documented spelling (for example `ssl-mode=VERIFY_IDENTITY`); unknown query parameters are rejected. See [release validation](docs/release-validation.md) for the checks required before publishing.

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
- [x] MySQL support
- [ ] Neovim integration (`sabiql.nvim`)
- [ ] Connection auto-detection (environment variables, URI)
- [ ] Google Cloud SQL / AlloyDB support

Have a feature request? [Open an issue](https://github.com/riii111/sabiql/issues/new). Feedback is welcome!

## License

MIT — see [LICENSE](LICENSE).
