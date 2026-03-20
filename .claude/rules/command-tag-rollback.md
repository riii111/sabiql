---
paths:
  - "**/src/domain/command_tag.rs"
  - "**/src/infra/adapters/postgres/psql/parser/**"
---

# CommandTag 設計と Rollback Policy

## 単一 enum 設計

CommandTag は display と refresh を単一 enum で自然に両立する。consumer はメソッドレベルで直交しており干渉しない:
- Display: `display_message()` — UI 層のみ
- Refresh: `needs_refresh()` / `is_schema_modifying()` — reducer + adapter aggregation

## Variant 追加チェックリスト

1. `display_message()` に表示文字列を定義
2. `needs_refresh()` — 永続化された状態を変更するなら true
3. `is_schema_modifying()` — スキーマ変更なら true
4. `is_data_modifying()` / `affected_rows()` を必要に応じて更新

## 再評価トリガー

この設計を見直す条件:
- multi-result 対応で `aggregate()` の戻り値が変わる
- display だけ必要な variant が出現する
- 新 consumer が display と refresh を同一メソッドで跨ぐ

## Rollback policy

psql の completion tag には savepoint 名が含まれないため、`discard_rolled_back` は depth ベースの近似で動作する。

設計判断: **false-positive-over-missed** — 曖昧なケースでは不要な refresh を許容し、必要な refresh を見逃さない方向に倒す。

## ResolvedTags read model

`ResolvedTags` は multi-statement 実行結果の read model。2つのビューを持つ:
- `all`: CTAS 補正済み（psql が本来報告すべきだった tag）
- `effective`: rollback filtering 後（実際に永続化された tag）

`aggregate()` が最終的な 1 tag を選択する。

## CTAS / SELECT INTO 補正

- CTAS と SELECT INTO に対して psql は `SELECT n` を返す
- `correct_ctas_tags` が rollback filtering の**前**に Select→Create 変換を行う
- これにより rolled-back な Create は `discard_rolled_back` で自然に破棄される
- 補正は adapter-local helper (`detect_ctas_kind`) で判定。app 層の classifier に依存しない
- SQL 文と tag の positional mapping が不一致の場合は補正をスキップする。これは general policy（missed refresh を避ける）の例外で、存在しないテーブルの Create 表示を防ぐために refresh の見逃しを許容する判断
