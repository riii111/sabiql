---
paths:
  - "**/src/app/ports/**/*.rs"
  - "**/src/infra/adapters/**/*.rs"
---

# DB 非依存ルール

## Port レベルの中立性

- `app/ports/` の port trait は RDBMS 非依存の汎用型・構文のみ使用する。理由: MySQL adapter が同一 port を変更なしで実装できるようにするため
- port メソッドのシグネチャは汎用型を使うこと（`PgType` や PG 固有 enum は不可）
- port のドキュメントは RDBMS 非依存の振る舞いとして記述する

## Adapter の分離

- DB 固有の SQL、クォート、接続文字列ロジックはすべて `infra/adapters/{postgres,mysql}/` に配置すること
- Adapter は方言固有の型を port の戻り値型に漏洩させてはならない
- 一方の adapter に機能を追加したら、もう一方の adapter 用にトラッキング Issue を作成すること

## 拡張準備手順

port trait を変更する際:
1. 新しいメソッドシグネチャが方言中立であることを確認
2. 既存の PG adapter 実装が抽象化すべき PG 固有構文を使っていないかチェック
3. MySQL adapter スタブが存在する場合、コンパイルが通ることを確認（`#[ignore]` テストでも可）

## Adapter 内部サブモジュール規約

各 adapter ディレクトリは `mod.rs`（オーケストレーション）+ CLI ディレクトリ（プロセス実行・パース）+ `sql/`（純粋SQL生成）の3層構造に従う。PG 固有の詳細は `postgres-adapter.md` を参照。

**MySQL adapter を追加する場合**、PG adapter の構造をミラーすること: `mysql/mod.rs`, `mysql/mysql_cli/`, `mysql/sql/` 等。

## 現在の Adapter 状況

- PostgreSQL: メイン、完全実装済み
- MySQL: 計画中、未実装
