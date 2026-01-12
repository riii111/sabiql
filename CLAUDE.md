# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**sabiql** is a fast, driver-less TUI for browsing PostgreSQL databases.

- **Tech Stack**: Rust + Ratatui + Tokio
- **Target DB**: PostgreSQL (MVP), MySQL (future)
- **No DB Driver Required**: Uses `psql`/`mysql` CLI for queries (driver-less architecture)

## Setup

```bash
mise install                       # Install tools (Rust, lefthook, etc.)
mise exec -- lefthook install      # Set up Git hooks (runs cargo fmt on commit)
```

## Build Commands

```bash
mise run build              # Build
mise run clippy             # Lint
mise run fmt                # Format
mise run test               # Run tests
```

## Configuration

- Connection config: `~/.config/sabiql/connections.toml`
- Cache directory: `~/.cache/sabiql/<project>/`

## Rules and Skills

Rules are stored in `.claude/rules/` and are **automatically loaded** based on file paths.
Skills are stored in `.claude/skills/` and must be **manually invoked**.

### Available Rules

| Rule | Applies to | Description |
|------|-----------|-------------|
| **architecture** | `**/src/**/*.rs` | Hexagonal architecture, layer structure, dependency rules, ports & adapters |
| **ui-design** | `**/src/ui/**/*.rs` | Atomic Design pattern, footer hint ordering, keybindings |
| **rust-testing** | `**/*_test*.rs`, `**/tests/**/*.rs` | Test structure, naming conventions, rstest usage |
| **visual-regression** | `**/tests/render_snapshots.rs` | Snapshot testing with insta, coverage criteria |

### Available Skills

| Skill | Description |
|-------|-------------|
| **release** | Version bump, tag creation, GitHub release workflow |
