# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**sabiql** is a fast, driver-less TUI for browsing and editing PostgreSQL databases.

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

Rules（`.claude/rules/`）は `paths` フロントマターのパターンに基づき**自動ロード**される。
Skills（`.claude/skills/`）は手動で `/skill-name` で呼び出す。

### Available Rules

**全体（`**/src/**/*.rs`）:**

| Rule | 説明 |
|------|------|
| **architecture** | ヘキサゴナルアーキテクチャ、レイヤ依存、Ports & Adapters、副作用境界 |
| **testing** | テスト命名・構造・rstest凝集度・レイヤ別カバレッジ義務・`#[ignore]` トラッキング |

**レイヤ・パス限定**（対象パスは概要。正確な glob は各ルールの frontmatter を参照）**:**

| Rule | 対象パス | 説明 |
|------|---------|------|
| **app-state** | `app/state.rs`, `app/reducers/**` | 派生状態パターン、aggregate不変条件、State/View分離 |
| **reducer-structure** | `app/reducer.rs`, `app/reducers/**` | Reducer feature 分割、Dispatcher パターン、aggregate-first |
| **effect-runner** | `app/effect_runner.rs`, `app/effect_handlers/**`, `app/effect.rs` | Dispatcher パターン、依存注入、RefCell borrow安全 |
| **interaction-contract** | keybindings, handler, footer, help, palette | SSOT整合性、キー変換フロー、チェックリスト |
| **nav-intent-design** | `app/nav_intent.rs`, `handlers/normal.rs` | NavIntent SSOT責務分離、NavigationContext |
| **ui-design** | `src/ui/**` | Atomic Design、フッター順序、テキスト入力 |
| **postgres-adapter** | `infra/adapters/postgres/**` | データフロー、可視性ルール |
| **command-tag-rollback** | `domain/command_tag.rs`, `infra/**/parser/**` | CommandTag enum設計、rollback近似方針 |
| **db-agnostic** | `app/ports/**`, `infra/adapters/**` | Port中立性、Adapter分離、MySQL準備 |
| **visual-regression** | `tests/render_snapshots/**` | instaスナップショット、モードカバレッジ |
| **rendering-strategy** | `app/render_schedule.rs`, `main.rs` | イベント駆動レンダリング、トリガー表 |
| **config-migration** | config_writer, connection_store | 後方互換スキーマ変更 |

### Available Skills

| Skill | Type | Description |
|-------|------|-------------|
| **release** | Manual | バージョンバンプ、タグ作成、GitHub リリース |
