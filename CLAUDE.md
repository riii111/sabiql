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
| **rust-testing-style** | テスト命名、構造、given/when/then |
| **testing-obligations** | レイヤ別必須テスト、`#[ignore]` トラッキング |

**レイヤ・パス限定**（対象パスは概要。正確な glob は各ルールの frontmatter を参照）**:**

| Rule | 対象パス | 説明 |
|------|---------|------|
| **app-state** | `app/state.rs`, `app/reducers/**` | 派生状態パターン、State/View分離 |
| **postgres-adapter** | `infra/adapters/postgres/**` | Adapter内部構造、data flow、可視性 |
| **rendering-strategy** | `app/render_schedule.rs`, `main.rs` | イベント駆動レンダリング、トリガー表 |
| **ui-design** | `src/ui/**` | Atomic Design、フッター順序、テキスト入力 |
| **interaction-contract** | keybindings, handler, footer, help, palette | SSOT整合性、キー変換フロー、チェックリスト |
| **db-agnostic** | `app/ports/**`, `infra/adapters/**` | Port中立性、Adapter分離、MySQL準備 |
| **config-migration** | config_writer, connection_store | 後方互換スキーマ変更 |
| **rstest-patterns** | `domain/**`, `infra/**/parser*`, `infra/**/sql/**` | rstest凝集度、境界値パターン |
| **test-organization** | `app/**`, `ui/event/**` | mod構造、フィクスチャ抽出 |
| **visual-regression** | `tests/render_snapshots.rs`, `tests/snapshots/**` | instaスナップショット、モードカバレッジ |
| **effect-runner** | `app/effect_runner.rs`, `app/effect_handlers/**`, `app/effect.rs` | Dispatcher パターン、依存注入、新 Effect 追加手順 |
| **reducer-structure** | `app/reducer.rs`, `app/reducers/**` | Reducer feature 分割、Dispatcher パターン、passthrough、新 Action / ConfirmIntent 追加手順 |
| **rollback-policy** | `infra/**/parser.rs` | command tag 集約の rollback 近似方針、false-positive-over-missed |
| **command-tag-design** | `domain/command_tag.rs` | 単一 enum での display/refresh 兼用根拠、variant 追加チェックリスト |
| **nav-intent-design** | `app/nav_intent.rs`, `handlers/normal.rs` | NavIntent SSOT責務分離、NavigationContext、キー追加チェックリスト |

### Available Skills

| Skill | Type | Description |
|-------|------|-------------|
| **release** | Manual | バージョンバンプ、タグ作成、GitHub リリース |
