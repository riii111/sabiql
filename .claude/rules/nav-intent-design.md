---
paths:
  - "**/src/app/update/input/nav_intent.rs"
  - "**/src/ui/event/handlers/normal.rs"
---

# NavIntent 設計ルール

## SSOT 責務分離

- `app/update/input/keybindings/` = 入力構文と表示（SSOT）
- `app/update/input/nav_intent.rs` = vim-like navigation の意味論（NavIntent）と文脈適用（resolve）
- `ui/event/handlers/normal.rs` = モードディスパッチ（NavIntent 対象キーは `resolve_nav` 経由）

## NavIntent 対象範囲

1つのキーが `NavigationContext` で異なる Action に分岐するキーのみ。単発 action（q, ?, :, s 等）や mode 遷移（Esc, Enter, y, d 等）は対象外。

## NavigationContext

`AppState` から `from_state()` で派生する。NavIntent 対象キーについて、handler で直接 state を参照して文脈分岐してはならない。

5 variant: `Explorer`, `Inspector`, `ResultScroll`, `ResultRowActive`, `ResultCellActive`

## 新規 vim-like ナビゲーションキー追加手順

1. `NavIntent` に variant 追加
2. `map_nav_intent()` に KeyCombo → NavIntent マッピング追加
3. `resolve()` に全 NavigationContext × NavIntent の match arm 追加
4. `keybindings/normal.rs` に display-only エントリ追加
5. matrix テスト（`handlers/normal.rs`）に全 context のテストケース追加
6. `resolve()` のユニットテスト（`nav_intent.rs`）に新 intent の全 context パターン追加
