---
paths:
  - "**/src/infra/adapters/postgres/**/*.rs"
---

# Postgres Adapter 内部構造

## 構造とデータフロー

`mod.rs`（オーケストレーション）→ `sql/`（SQL 生成、純粋関数）→ `psql/executor.rs`（psql プロセス実行）→ `psql/parser/`（stdout → ドメイン型変換、純粋関数）

## 可視性ルール

- 関数はデフォルト private
- サブモジュール間アクセスには `pub(in crate::infra::adapters::postgres)` を使う
- テストは各サブモジュール内に `#[cfg(test)]` で配置

## クォート関数

`crate::infra::utils::{quote_ident, quote_literal}` を使うこと。`pg_quote_*` のような重複関数を作ってはならない。
