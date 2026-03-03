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

Rules are stored in `.claude/rules/` and are **automatically loaded** based on `paths` frontmatter patterns.
Skills are stored in `.claude/skills/`:
- **Manual** skills: only user can invoke via `/skill-name` (`disable-model-invocation: true`)
- **Auto** skills: Claude fires automatically based on conversation context
  - `user-invocable: false` additionally hides the skill from the `/` menu

### Available Rules

| Rule | Applies to | Description |
|------|-----------|-------------|
| **architecture** | `**/src/**/*.rs` | Hexagonal architecture, layer deps, ports & adapters, side-effect boundaries |
| **ui-design** | `**/src/ui/**/*.rs` | Atomic Design pattern, footer hint ordering, keybindings |
| **interaction-contract** | keybindings, event handler, footer, help, palette files | Keybinding SSOT consistency, full checklist |
| **db-agnostic** | `**/src/app/ports/**`, `**/src/infra/adapters/**` | Port neutrality, adapter isolation, MySQL readiness |
| **config-migration** | config_writer, connection_store files | Backward-compatible config schema changes |
| **rust-testing-style** | `**/src/**/*.rs`, `**/tests/**/*.rs` | Test naming, structure, rstest usage |
| **testing-obligations** | `**/src/**/*.rs`, `**/tests/**/*.rs` | MUST-test targets by layer, `#[ignore]` tracking |
| **visual-regression** | `**/tests/render_snapshots.rs`, `**/tests/snapshots/**` | insta snapshots, mode coverage obligation |

### Available Skills

| Skill | Type | Description |
|-------|------|-------------|
| **release** | Manual | Version bump, tag creation, GitHub release workflow |
