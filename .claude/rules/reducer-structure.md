---
paths:
  - "**/src/app/reducer.rs"
  - "**/src/app/reducers/**/*.rs"
---

# Reducer 構造ルール

## ディレクトリ構造

```
src/app/
├── reducer.rs           # Dispatch chain
└── reducers/
    ├── mod.rs           # re-exports
    ├── helpers.rs       # crate 全体の共有ロジック
    ├── navigation.rs    # Focus/Pane, Inspector, Explorer, Filter, CommandLine, Paste
    ├── connection/
    │   ├── mod.rs       # Dispatcher のみ
    │   ├── lifecycle.rs # TryConnect, SwitchConnection
    │   ├── setup.rs     # フォーム入力 + Paste(ConnectionSetup) + Save/Cancel
    │   ├── error.rs     # エラー表示・スクロール・コピー・リトライ
    │   ├── selector.rs  # OpenConnectionSelector, 削除・編集
    │   └── helpers.rs   # cache save/restore ヘルパー (pub(super))。状態リセットは BrowseSession::reset() を使う
    ├── result/
    │   ├── mod.rs       # Dispatcher のみ
    │   ├── scroll.rs    # ResultScroll* + 共有ヘルパー
    │   ├── selection.rs # ResultEnter/Exit*, ResultCell*, Delete staging, NextPage/PrevPage passthrough
    │   ├── edit.rs      # ResultCellEdit*
    │   ├── yank.rs      # ResultCellYank, ResultRowYank, DdlYank, CellCopied, CopyFailed
    │   └── history.rs   # ResultHistory 操作
    └── ...              # query.rs, modal.rs, metadata.rs, er.rs, sql_modal.rs
```

## Dispatcher パターン

`result/mod.rs` および `connection/mod.rs` は dispatcher のみ。ロジックは各 `<feature>.rs` に配置する。

Connection 系サブ reducer 間に passthrough 依存はない（dispatcher 順序は任意）。

## サブモジュール間共有ヘルパー

- サブモジュール間（result, connection）: `pub(super)` で公開
- crate 全体: `reducers/helpers.rs` に配置

## Passthrough パターン

あるサブ reducer が状態リセットだけ行い `None` を返して後続 reducer に委ねるケース（例: `ResultNextPage/PrevPage`）。
このパターンを使う場合、「リセットされること」と「後続 reducer が effect を生成すること」の両方をテストすること。

**順序依存**: passthrough は `reduce_result` が `reduce_query` より前に dispatch される前提に依存する。chain を並べ替える際はこの依存を壊さないこと。

## navigation.rs の境界

以下は navigation.rs に残す（小規模・自己完結のため）:
- Focus / Pane 移動
- Inspector スクロール・タブ切り替え
- Explorer ナビゲーション
- Filter / CommandLine
- Paste
- ConnectionList 操作

Result 系ロジックを navigation.rs に追加してはならない。

## Result pane 表示切り替え時の不変条件

- **view state リセットは `reset_result_view()` で一括実行すること**。scroll offset / selection / staged_delete_rows / pending_write_preview を個別に手書きしない。フィールド追加時の漏れを防ぐ。
- **`history_index` は user-initiated な履歴閲覧（Ctrl+H）専用**。adhoc completion から直接 set すると footer が history-browsing モードに切り替わり、ペイン切り替え等の Normal キーバインドが効かなくなる。
- **adhoc success → `current_result` に書いてよい**（Result pane に表示される）。**adhoc error → `current_result` に書かない**（`SqlModalContext.last_adhoc_error` に閉じ込め、既存 preview を保持する）。

## Aggregate-first パターン

Reducer は aggregate に遷移を委譲し、field の直接更新を最小限にする。

### 原則

- **co-dependent invariant のある field は aggregate メソッドで更新すること**。直接代入は不変条件の破綻を招く。
- **cross-cutting な reset/restore は aggregate 側で所有する**。例: `BrowseSession::reset()`, `SqlModalContext::reset_prefetch()`.
- UI display state / テキスト入力 / 意図的に public な field の直接操作は許容。

### 直接操作が許容される field

- `ResultInteraction` public fields（scroll_offset, horizontal_offset 等）
- `BrowseSession` public fields（dsn, active_connection_id 等）
- `SqlModalContext`: `content`, `cursor`, `completion.*`, `completion_debounce`, `prefetch_queue`, `prefetching_tables`, `failed_prefetch_tables`（読み取り・queue の push/pop は許容。lifecycle 操作は aggregate API を使う）
- `PaginationState` fields（current_page, reached_end）

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
