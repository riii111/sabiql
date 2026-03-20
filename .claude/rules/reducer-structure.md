---
paths:
  - "**/src/app/reducer.rs"
  - "**/src/app/reducers/**/*.rs"
---

# Reducer 構造ルール

## 構造

`reducer.rs` が dispatch chain → `reducers/{feature}.rs` にロジックを配置。`result/`, `connection/`, `query/`, `navigation/` は sub-dispatcher（`mod.rs` は dispatch のみ）。共有ヘルパーは `reducers/helpers.rs`（crate 全体）または各サブの `helpers.rs`（`pub(super)`）。

## Dispatcher パターン

`result/mod.rs`, `connection/mod.rs`, `query/mod.rs`, `navigation/mod.rs` は dispatcher のみ。ロジックは各 `<feature>.rs` に配置する。

Connection 系サブ reducer 間に passthrough 依存はない（dispatcher 順序は任意）。

## サブモジュール間共有ヘルパー

- サブモジュール間（result, connection, query）: `pub(super)` で公開
- crate 全体: `reducers/helpers.rs` に配置

## Passthrough パターン

あるサブ reducer が状態リセットだけ行い `None` を返して後続 reducer に委ねるケース（例: `ResultNextPage/PrevPage`）。
このパターンを使う場合、「リセットされること」と「後続 reducer が effect を生成すること」の両方をテストすること。

**順序依存**: passthrough は `reduce_result` が `reduce_query` より前に dispatch される前提に依存する。chain を並べ替える際はこの依存を壊さないこと。

## navigation/ の境界

`navigation/` は Focus/Pane (`focus.rs`)、Inspector スクロール (`inspector.rs`)、Explorer ナビゲーション (`explorer.rs`)、テキスト入力 (`input.rs`)、接続リスト (`connection_list.rs`) に分割。Result 系・Connection 系ロジックはそれぞれ `result/`, `connection/` に配置すること。

## Result pane 表示切り替え時の不変条件

- **view state リセットは `reset_result_view()` で一括実行すること**。scroll offset / selection / staged_delete_rows / pending_write_preview を個別に手書きしない。フィールド追加時の漏れを防ぐ。
- **`history_index` は user-initiated な履歴閲覧（Ctrl+H）専用**。adhoc completion から直接 set すると footer が history-browsing モードに切り替わり、ペイン切り替え等の Normal キーバインドが効かなくなる。
- **adhoc success → `current_result` に書いてよい**（Result pane に表示される）。**adhoc error → `current_result` に書かない**（`SqlModalContext.last_adhoc_error` に閉じ込め、既存 preview を保持する）。

## Aggregate-first パターン

Reducer は aggregate に遷移を委譲し、field の直接更新を最小限にする。理由: co-dependent field の個別更新は transient invalid state を生み、描画パニックの原因になる。

### 原則

- **co-dependent invariant のある field は aggregate メソッドで更新すること**。
- **cross-cutting な reset/restore は aggregate 側で所有する**。例: `BrowseSession::reset()`, `SqlModalContext::reset_prefetch()`.
- UI display state / テキスト入力 / 意図的に public な field の直接操作は許容。

### 直接操作が許容される field

各 aggregate の public fields（`ResultInteraction` の scroll_offset 等）と UI display state は直接操作可。lifecycle 操作や co-dependent fields は aggregate API を使うこと。

## 新 Action 追加時

1. `action.rs` に variant 追加
2. 対応する feature reducer の match arm に追加
3. テスト追加

Result 系 Action は `result/<feature>.rs` に追加する。`navigation.rs` には置かない。
Connection 系 Action は `connection/<feature>.rs` に追加する。完了通知 action は操作文脈（開始画面）のモジュールに置く。

## 新 ConfirmIntent 追加時

Confirm dialog は `ConfirmIntent` enum（`confirm_dialog_state.rs`）で workflow を型付けする。任意の `Action` を dialog state に保持してはならない。

1. `confirm_dialog_state.rs` の `ConfirmIntent` に variant 追加
2. dialog を開く reducer で `state.confirm_dialog.open(title, message, intent)` を使用
3. `modal.rs` の `ConfirmDialogConfirm` / `ConfirmDialogCancel` に match arm 追加
4. confirm 後に async effect を発行する場合、その完了ハンドラが参照する state（例: `pending_write_preview`）を confirm 時点でクリアしないこと
