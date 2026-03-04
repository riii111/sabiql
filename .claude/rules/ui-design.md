---
paths:
  - "**/src/ui/**/*.rs"
---

# UI設計ルール

## コンポーネント構造（Atomic Design）

UIコンポーネントは Atomic Design パターンに従う:

```
src/ui/components/
├── atoms/       # 最小の再利用単位（spinner, key_chip, panel_border）
├── molecules/   # atoms の組み合わせ（modal_frame, hint_bar）
└── *.rs         # Organisms: 画面レベルのコンポーネント（explorer, inspector 等）
```

| レイヤ | 用途 | 例 |
|--------|------|-----|
| atoms | 単一目的のプリミティブ | `spinner_char()`, `key_chip()`, `panel_block()`, `text_cursor_spans()` |
| molecules | atoms を組み合わせた再利用パターン | `render_modal()`, `hint_line()` |
| organisms | 画面セクション。molecules/atoms を使う | `Explorer`, `SqlModal`, `Footer` |

UIコンポーネント追加時:
- 繰り返し出現するビジュアルパターンは atoms/molecules に切り出す
- `Color::*` 直指定ではなく `Theme::*` トークンを使う
- Organisms は molecules/atoms を合成し、ロジックを複製しない

## 単一行テキスト入力

- **新規の**単一行テキスト入力フィールドはすべて `TextInputState`（`app/text_input.rs`）で状態管理すること
  - 既知の例外: `ConnectionSetupState` は現在独自に `cursor_position` / `viewport_offset` を管理している（マイグレーションは別途追跡）
- カーソル描画は `text_cursor_spans()`（`ui/components/atoms/text_cursor.rs`）を使うこと。インラインでカーソル描画ロジックを複製してはならない

## フッターヒント順序

すべての InputMode で以下の順序に従うこと:

```
アクション → ナビゲーション → ヘルプ → 閉じる/キャンセル → 終了
```

## インタラクション契約

キーバインドの整合性ルールと追加チェックリストは `interaction-contract.md` を参照。
