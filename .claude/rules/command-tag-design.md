---
paths:
  - "**/src/domain/command_tag.rs"
---

# CommandTag 設計: 単一 enum による display/refresh 兼用

## 判断 (SAB-142)

`CommandTag` は display と refresh の両責務を単一 enum で担う。型分割は行わない。

根拠: consumer が完全に分離しており、共有メソッドがない。
- Display: `display_message()` — UI 層のみ
- Refresh: `needs_refresh()` / `is_schema_modifying()` — app 層 reducer + adapter aggregation

`ResolvedTags::aggregate()` は refresh 系メソッドのみで tag を選択し、選択された tag の `display_message()` は常に意味的に正しい。

## Variant 追加チェックリスト

新しい variant を追加するとき:
1. `display_message()` に対応する表示文字列を定義
2. `needs_refresh()` に含めるか判断（永続化された状態を変更するか）
3. `is_schema_modifying()` に含めるか判断（スキーマ変更か）
4. `is_data_modifying()` に含めるか判断
5. `affected_rows()` が適用可能か判断

## 再評価トリガー

以下のいずれかが発生したら、display/refresh の型分離を再検討する:
- multi-result 対応で `aggregate()` の戻り値が変わる場合
- display だけ必要（refresh 不要）な variant が出現した場合
- 新 consumer が display と refresh を同一メソッドで跨ぐ場合
