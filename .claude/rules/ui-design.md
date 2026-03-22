---
paths:
  - "**/src/ui/**/*.rs"
---

# UI設計ルール

## コンポーネント構造

UIは shell / features / primitives の3層構成に従う:

```
src/ui/
├── shell/        # 常時描画される画面骨格（layout, header, footer, command_line）
├── features/     # feature/mode 単位のコンポーネント
│   ├── browse/         # Normal mode ペイン（explorer, inspector, result）
│   ├── connections/    # 接続管理（setup, selector, error）
│   ├── sql_modal/      # SQL編集モーダル
│   ├── pickers/        # テーブル/コマンドピッカー
│   └── overlays/       # オーバーレイ（help, confirm_dialog）
└── primitives/   # feature 文脈を持たない UI 基礎部品
    ├── atoms/    # それ以上分解できない単一目的部品（spinner, key_chip, panel_block 等）
    ├── molecules/# atoms の組み合わせパターン（render_modal, overlay helpers 等）
    └── utils/    # 描画なし計算ユーティリティ（text_utils）
```

| レイヤ | 用途 | 例 |
|--------|------|-----|
| shell | 画面骨格。常時描画 | `MainLayout`, `Header`, `Footer`, `CommandLine` |
| features | feature/mode 単位の画面セクション | `Explorer`, `SqlModal`, `HelpOverlay` |
| primitives/atoms | それ以上分解できない単一目的部品 | `spinner_char()`, `key_chip()`, `panel_block()`, `text_cursor_spans()` |
| primitives/molecules | atoms を組み合わせた描画パターン | `render_modal()`, `hint_line()` |
| primitives/utils | 描画なし計算ユーティリティ | `calculate_header_min_widths()` |

### primitives 配置基準

分類軸は**使用数ではなく責務と文脈依存性**なのだ。

**primitives に置く条件（すべて満たすこと）:**
- `AppState` や feature 固有の状態を直接受け取らない
- mode 分岐を持たない
- feature 名・画面名を含む語彙を持たない
- ステートレス（呼び出し側が状態を渡す）

**primitives に置かない条件（いずれか該当）:**
- feature 固有の文脈がないと意味をなさない
- 近い将来その feature 都合で変わりそう
- feature 内に置いたほうが意味が明瞭

UIコンポーネント追加時:
- 繰り返し出現するビジュアルパターンは primitives/atoms または primitives/molecules に切り出す
- `Color::*` 直指定ではなく `Theme::*` トークンを使う
- features/ コンポーネントは primitives/ を合成し、ロジックを複製しない

## 単一行テキスト入力

- **新規の**単一行テキスト入力フィールドはすべて `TextInputState`（`app/model/shared/text_input.rs`）で状態管理すること
  - 既知の例外: `ConnectionSetupState` は現在独自に `cursor_position` / `viewport_offset` を管理している（マイグレーションは別途追跡）
- カーソル描画は `text_cursor_spans()`（`ui/primitives/atoms/text_cursor.rs`）を使うこと。インラインでカーソル描画ロジックを複製してはならない

## フッターヒント順序

すべての InputMode で以下の順序に従うこと:

```
アクション → ナビゲーション → ヘルプ → 閉じる/キャンセル → 終了
```

## インタラクション契約

キーバインドの整合性ルールと追加チェックリストは `interaction-contract.md` を参照。
