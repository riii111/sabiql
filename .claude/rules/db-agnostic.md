---
paths:
  - "**/src/app/ports/**/*.rs"
  - "**/src/infra/adapters/**/*.rs"
---

# DB 非依存ルール

## Port レベルの中立性（必須）

- `app/ports/` の port trait に PostgreSQL 固有の SQL や構文を含めてはならない
- port メソッドのシグネチャは汎用型を使うこと（`PgType` や PG 固有 enum は不可）
- port のドキュメントは特定の RDBMS を参照せずに振る舞いを記述すること

## Adapter の分離（必須）

- DB 固有の SQL、クォート、接続文字列ロジックはすべて `infra/adapters/{postgres,mysql}/` に配置すること
- Adapter は方言固有の型を port の戻り値型に漏洩させてはならない
- 一方の adapter に機能を追加したら、もう一方の adapter 用にトラッキング Issue を作成すること

## 拡張準備チェックリスト

port trait を変更する際:
1. 新しいメソッドシグネチャが方言中立であることを確認
2. 既存の PG adapter 実装が抽象化すべき PG 固有構文を使っていないかチェック
3. MySQL adapter スタブが存在する場合、コンパイルが通ることを確認（`#[ignore]` テストでも可）

## Adapter 内部サブモジュール規約

各 adapter ディレクトリ（例: `postgres/`）は以下の構造に従う:

- **`mod.rs`**: 構造体定義 + port trait 実装（`MetadataProvider`, `QueryExecutor`）。オーケストレーションのみ — SQL 生成やパースロジックは置かない。
- **`psql/`（または `mysql/` CLI ディレクトリ）**: プロセス実行（`executor.rs`）と出力パース（`parser.rs`）。副作用は `executor.rs` に限定。
- **`sql/`**: 純粋な SQL 文字列生成。関心事で分割: `query.rs`（メタデータ）、`ddl.rs`（DDL）、`dialect.rs`（DML）。
- **ユーティリティモジュール**（`select_guard.rs`, `dsn.rs`）: 単一目的の純粋関数。

**MySQL adapter を追加する場合**、この構造をミラーすること: `mysql/mod.rs`, `mysql/mysql_cli/`, `mysql/sql/` 等。port trait 実装は `mod.rs` に、方言固有 SQL は `sql/` に配置。

## 現在の Adapter 状況

- PostgreSQL: メイン、完全実装済み
- MySQL: 計画中、未実装
