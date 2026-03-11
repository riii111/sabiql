---
paths:
  - "**/src/app/reducer.rs"
  - "**/src/app/reducers/**/*.rs"
---

# Reducer 構造ルール

## ディレクトリ構造

```
src/app/
├── reducer.rs           # Dispatch chain（reduce_connection → reduce_modal → reduce_result → reduce_navigation → ...）
└── reducers/
    ├── mod.rs           # re-exports
    ├── helpers.rs       # crate 全体で使う共有ロジック（build_bulk_delete_preview 等）
    ├── navigation.rs    # Focus/Pane, Inspector, Explorer, Filter, CommandLine, Paste 等
    ├── connection/
    │   ├── mod.rs       # Dispatcher のみ（.or_else() チェーン、passthrough なし・順序任意）
    │   ├── lifecycle.rs # TryConnect, SwitchConnection
    │   ├── setup.rs     # フォーム入力全般 + Paste(ConnectionSetup) + Save/Cancel
    │   ├── error.rs     # エラー表示・スクロール・コピー・リトライ
    │   ├── selector.rs  # OpenConnectionSelector, 削除・編集リクエスト
    │   └── helpers.rs   # save_current_cache, restore_cache, reset_connection_state (pub(super))
    ├── result/
    │   ├── mod.rs       # Dispatcher のみ（.or_else() チェーン）
    │   ├── scroll.rs    # ResultScroll* + 共有ヘルパー（result_row_count 等）
    │   ├── selection.rs # ResultEnter/Exit*, ResultCell*, Delete staging, NextPage/PrevPage passthrough
    │   ├── edit.rs      # ResultCellEdit*
    │   ├── yank.rs      # ResultCellYank, ResultRowYank, DdlYank, CellCopied, CopyFailed
    │   └── history.rs   # OpenResultHistory, History{Older,Newer}, ExitResultHistory
    └── ...              # query.rs, modal.rs, metadata.rs, er.rs, sql_modal.rs
```

## Dispatcher パターン

`result/mod.rs` および `connection/mod.rs` は dispatcher のみ。ロジックは各 `<feature>.rs` に配置する。

Connection 系サブ reducer 間に passthrough 依存はない（dispatcher 順序は任意）。

## サブモジュール間共有ヘルパー

- result サブモジュール間: `pub(super)` で公開（例: `scroll.rs` の `result_row_count`）
- connection サブモジュール間: `pub(super)` で公開（例: `helpers.rs` の `reset_connection_state`）
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

## 新 Action 追加時

1. `action.rs` に variant 追加
2. 対応する feature reducer の match arm に追加
3. テスト追加

Result 系 Action は `result/<feature>.rs` に追加する。`navigation.rs` には置かない。
Connection 系 Action は `connection/<feature>.rs` に追加する。完了通知 action は操作文脈（開始画面）のモジュールに置く。
