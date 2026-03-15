---
paths:
  - "**/src/infra/adapters/postgres/psql/parser.rs"
---

# Command tag aggregation rollback policy

## psql の制約

psql の completion tag には savepoint 名が含まれない（`SAVEPOINT` / `RELEASE` / `ROLLBACK` はすべて bare）。
そのため `discard_rolled_back` は depth ベースの近似で動作する。

## 設計判断: false-positive-over-missed

曖昧なケースでは **不要な refresh を許容し、必要な refresh を見逃さない** 方向に倒す。

## ResolvedTags read model

`ResolvedTags` は multi-statement 実行結果の read model。2つのビューを持つ:
- `all`: CTAS 補正済み（psql が本来報告すべきだった tag）
- `effective`: rollback filtering 後（実際に永続化された tag）

`aggregate()` が最終的な 1 tag を選択する。

## CTAS / SELECT INTO 補正

- CTAS (`CREATE TABLE ... AS SELECT`) と SELECT INTO に対して psql は `SELECT n` を返す
- `correct_ctas_tags` が rollback filtering の **前** に Select→Create 変換を行う
- これにより rolled-back な Create は `discard_rolled_back` で自然に破棄され、復活しない
- 補正は adapter-local helper (`detect_ctas_kind`) で判定。app 層の classifier に依存しない
- SQL 文と tag の positional mapping が不一致の場合は補正スキップ（安全 fallback: 補正失敗時は refresh を見逃す方向に倒れる。これは general policy の逆だが、「存在しないテーブルの Create 表示」を避ける判断）
