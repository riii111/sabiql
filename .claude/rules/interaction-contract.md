---
paths:
  - "**/src/app/keybindings/**/*.rs"
  - "**/src/app/keymap.rs"
  - "**/src/ui/event/**/*.rs"
  - "**/src/ui/components/footer.rs"
  - "**/src/ui/components/help_overlay.rs"
  - "**/src/app/palette.rs"
---

# インタラクション契約

## 唯一の信頼できる情報源（必須）

- `app/keybindings/` がすべてのキーバインドの **SSOT**
- フッターヒント、ヘルプオーバーレイ、コマンドパレットはキーバインドデータから派生させること
- `keybindings/` で宣言されていないキーコンボを `handler.rs` に定義してはならない

## 整合性の不変条件（必須）

1. `KeyBinding` / `ModeRow` エントリに表示ラベルがあれば、ヘルプオーバーレイに必ず表示する
2. フッターに表示するキーバインドは `handler.rs` のアクションに必ず解決できる
3. コマンドパレットのエントリはキーバインドと同じアクション名にマッピングする

## キー変換フロー

```
crossterm::KeyEvent
  → ui/event/key_translator::translate()
  → app::keybindings::KeyCombo
  → app::keymap::resolve(combo, bindings)   (simple modes)
     OR keybindings::is_quit(&combo) 等     (Normal mode predicates)
  → Action
```

**責務分担:**
- `app/keybindings/`: SSOT モジュール — `KeyBinding`（simple modes）と `ModeRow`（mixed modes）。サブモジュール: `normal.rs`, `overlays.rs`, `connections.rs`, `editors.rs`, `types.rs`。Mixed modes は `ModeBindings { rows: &[ModeRow] }` を使い `.resolve()` で解決
- `app/keymap.rs`: `KeyBinding` スライス用の `resolve(combo, bindings)` と `ModeRow` スライス用の `resolve_mode(combo, rows)`
- `ui/event/key_translator.rs`: UI adapter — `crossterm::KeyEvent` → app 層の `KeyCombo` に変換
- `ui/event/handler.rs`: モードディスパッチ — `ModeBindings::resolve()` または predicate fn を呼び出し、コンテキストロジックを適用

**Char フォールバックルール**: フリーテキスト入力のあるモード（TablePicker, ErTablePicker, CommandLine, CellEdit）は `keymap::resolve()` を先に試し、その後 `Char(c)` にフォールスルーする。これらのモードにコマンドキーとして `KeyCombo::plain(Key::Char(x))` を追加してはならない。非 Char キー（Up/Down/Esc/Enter）を使うこと。

## 新規キーバインド追加チェックリスト

1. `app/keybindings/{normal,overlays,connections,editors}.rs` にエントリ追加
2. Normal mode の場合: `keybindings/mod.rs` に predicate fn を追加 + `handler.rs` で配線
3. ModeBindings mode の場合: `ModeRow` エントリを追加（ディスパッチは自動）
4. バインドをフッターに表示する場合: `display_hint` を更新
5. 該当モードのヘルプオーバーレイセクションを更新
6. パレットに表示すべきアクションなら `app/palette.rs` に追加
7. スナップショットテストを実行してフッター/ヘルプの描画を確認

## アンチパターン（禁止）

- `keybindings/` エントリなしに `handler.rs` にハードコードしたキーチェック
- キーバインドの表示ラベルと一致しないフッターヒントテキスト
- 対応するキーバインドエントリがないキーをヘルプオーバーレイに記載
