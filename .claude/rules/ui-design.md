---
paths:
  - "**/src/ui/**/*.rs"
---

# UI設計ルール

## コンポーネント構造

UIは shell / features / shared の3層構成に従う:

```
src/ui/
├── shell/        # 常時描画される画面骨格（layout, header, footer, command_line）
├── features/     # feature/mode 単位のコンポーネント
│   ├── browse/         # Normal mode ペイン（explorer, inspector, result）
│   ├── connections/    # 接続管理（setup, selector, error）
│   ├── sql_modal/      # SQL編集モーダル
│   ├── pickers/        # テーブル/コマンドピッカー
│   └── overlays/       # オーバーレイ（help, confirm_dialog）
└── shared/       # 2+ features で再利用するプリミティブ
    ├── atoms/    # 単一目的プリミティブ（spinner, key_chip, panel_block, scroll_indicator 等）
    ├── molecules/# atoms の組み合わせ（render_modal, hint_line, overlay helpers）
    └── utils/    # 描画なし計算ユーティリティ（text_utils）
```

| レイヤ | 用途 | 例 |
|--------|------|-----|
| shell | 画面骨格。常時描画 | `MainLayout`, `Header`, `Footer`, `CommandLine` |
| features | feature/mode 単位の画面セクション | `Explorer`, `SqlModal`, `HelpOverlay` |
| shared/atoms | 単一目的のプリミティブ（ステートレス） | `spinner_char()`, `key_chip()`, `panel_block()`, `text_cursor_spans()` |
| shared/molecules | atoms を組み合わせた再利用パターン | `render_modal()`, `hint_line()` |
| shared/utils | 描画なし計算ユーティリティ | `calculate_header_min_widths()` |

### shared 抽出基準

| 条件 | 分類先 |
|------|--------|
| 2+ features（または 1 feature + shell）で使用 & 単一目的 & ステートレス | `shared/atoms/` |
| 2+ features（または 1 feature + shell）で使用 & atoms を組み合わせたパターン | `shared/molecules/` |
| 2+ features で使用 & 計算ユーティリティ（描画なし） | `shared/utils/` |
| 1 feature でのみ使用 | feature 内に留める |

UIコンポーネント追加時:
- 繰り返し出現するビジュアルパターンは shared/atoms または shared/molecules に切り出す
- `Color::*` 直指定ではなく `Theme::*` トークンを使う
- features/ コンポーネントは shared/ を合成し、ロジックを複製しない

## 単一行テキスト入力

- **新規の**単一行テキスト入力フィールドはすべて `TextInputState`（`app/text_input.rs`）で状態管理すること
  - 既知の例外: `ConnectionSetupState` は現在独自に `cursor_position` / `viewport_offset` を管理している（マイグレーションは別途追跡）
- カーソル描画は `text_cursor_spans()`（`ui/shared/atoms/text_cursor.rs`）を使うこと。インラインでカーソル描画ロジックを複製してはならない

## フッターヒント順序

すべての InputMode で以下の順序に従うこと:

```
アクション → ナビゲーション → ヘルプ → 閉じる/キャンセル → 終了
```

## インタラクション契約

キーバインドの整合性ルールと追加チェックリストは `interaction-contract.md` を参照。
