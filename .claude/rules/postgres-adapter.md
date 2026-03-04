---
paths:
  - "**/src/infra/adapters/postgres/**/*.rs"
---

# Postgres Adapter 内部構造

## ディレクトリ構成

```
src/infra/adapters/postgres/
├── mod.rs              # PostgresAdapter構造体 + MetadataProvider / QueryExecutor impl
│                       # （オーケストレーション: sql/ でSQL生成 → psql/ で実行・パース）
├── psql/               # psql プロセス操作
│   ├── mod.rs          #   re-exports
│   ├── executor.rs     #   プロセス起動（I/O、副作用あり）
│   └── parser.rs       #   stdout → ドメイン型への変換（純粋関数）
├── sql/                # SQL文字列生成（すべて純粋関数）
│   ├── mod.rs          #   re-exports
│   ├── query.rs        #   メタデータクエリ + プレビュー
│   ├── ddl.rs          #   DDL生成（CREATE TABLE）
│   └── dialect.rs      #   DML生成（UPDATE/DELETE）
├── select_guard.rs     # SELECT安全チェック（純粋関数）
└── dsn.rs              # DSN構築
```

## データフロー

`mod.rs` がオーケストレーション → `sql/` でSQL生成 → `psql/executor.rs` で psql 実行 → `psql/parser.rs` で出力パース

## 可視性ルール

- 関数はデフォルト private
- サブモジュール間アクセスには `pub(in crate::infra::adapters::postgres)` を使う
- テストは各サブモジュール内に `#[cfg(test)]` で配置

## クォート関数

`crate::infra::utils::{quote_ident, quote_literal}` を使うこと。`pg_quote_*` のような重複関数を作ってはならない。
