---
paths:
  - "**/src/domain/command_tag.rs"
---

# CommandTag は display と refresh を自然に両立する単一 enum

consumer はメソッドレベルで直交しており、display/refresh が干渉しない設計になっている。
- Display: `display_message()` — UI 層のみ
- Refresh: `needs_refresh()` / `is_schema_modifying()` — reducer + adapter aggregation

## Variant 追加時

1. `display_message()` に表示文字列を定義
2. `needs_refresh()` — 永続化された状態を変更するなら true
3. `is_schema_modifying()` — スキーマ変更なら true
4. `is_data_modifying()` / `affected_rows()` を必要に応じて更新

## 再評価トリガー

この設計を見直す条件:
- multi-result 対応で `aggregate()` の戻り値が変わる
- display だけ必要な variant が出現する
- 新 consumer が display と refresh を同一メソッドで跨ぐ
