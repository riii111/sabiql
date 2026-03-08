---
paths:
  - "**/src/app/effect_runner.rs"
  - "**/src/app/effect_handlers/**/*.rs"
  - "**/src/app/effect.rs"
---

# Effect 実行ルール

## ディレクトリ構造

```
src/app/
├── effect.rs              # Effect enum 定義
├── effect_runner.rs       # Dispatcher（run / run_single / run_normal）+ EffectRunner builder
└── effect_handlers/
    ├── mod.rs             # EffectContext 定義 + re-exports
    ├── connection.rs      # SaveAndConnect, LoadConnectionForEdit, LoadConnections, DeleteConnection, SwitchConnection, SwitchToService
    ├── metadata.rs        # FetchMetadata, FetchTableDetail, PrefetchTableDetail, ProcessPrefetchQueue, DelayedProcessPrefetchQueue, CacheInvalidate
    ├── query.rs           # ExecutePreview, ExecuteAdhoc, ExecuteWrite, CountRowsForExport, ExportCsv
    ├── er.rs              # GenerateErDiagramFromCache, WriteErFailureLog, ExtractFkNeighbors, SmartErRefresh
    ├── completion.rs      # CacheTableInCompletionEngine, EvictTablesFromCompletionCache, ClearCompletionEngineCache, ResizeCompletionCache, TriggerCompletion
    └── test_support.rs    # Noop* stubs, make_runner(), sample helpers (#[cfg(test)])
```

## Dispatcher パターン

`effect_runner.rs` は **dispatcher のみ**。Effect のビジネスロジックは `effect_handlers/<feature>.rs` に配置する。
`effect_runner.rs` に inline で残すのは Render（`tui: &mut T` が必要）、CopyToClipboard、OpenFolder、Sequence、DispatchActions のみ。

## EffectContext

ポートの借用バンドル。新しい port を追加する場合は `EffectRunner` struct と `EffectContext` 両方にフィールドを追加し、`effect_context()` メソッドも更新する。

## RefCell borrow 安全ルール

`completion_engine` は `RefCell` なので EffectContext に含めない。必要な handler のみ引数で受ける。
**borrow は必ず await の前に drop すること**（ブロックスコープで囲む）。

```rust
// OK
let data = {
    let engine = completion_engine.borrow();
    engine.table_details_iter().map(|...| ...).collect::<Vec<_>>()
};
some_async_op(data).await;

// NG — borrow が await をまたぐ
let engine = completion_engine.borrow();
some_async_op(engine.data()).await; // panic at runtime
```

## 新 Effect 追加チェックリスト

1. `effect.rs` に variant 追加
2. 対応する `effect_handlers/<feature>.rs` の match arm に追加
3. `effect_runner.rs` の dispatcher match arm に追加（既存の `e @` パターンに追記）
4. handler のテストを追加
