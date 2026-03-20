---
paths:
  - "**/src/app/effect_runner.rs"
  - "**/src/app/effect_handlers/**/*.rs"
  - "**/src/app/effect.rs"
---

# Effect 実行ルール

## 構造

`effect.rs`（enum 定義）→ `effect_runner.rs`（dispatcher のみ）→ `effect_handlers/<feature>.rs`（ビジネスロジック）。

## Dispatcher パターン

`effect_runner.rs` は **dispatcher のみ**。Effect のビジネスロジックは `effect_handlers/<feature>.rs` に配置する。
`effect_runner.rs` に inline で残すのは Render（`tui: &mut T` が必要）、Sequence、DispatchActions のみ。

## 依存注入ルール

各 handler は **必要な port のみ引数で受け取る**。全 handler 共通のコンテキスト構造体は使わない。

- `action_tx` は全 handler 共通。シグネチャ先頭に置く
- その後に handler 固有の port を並べ、最後に `state` / `completion_engine`
- 返り値は `Result<()>` に統一
- 新しい port を追加する場合は `EffectRunner` struct にフィールドを追加し、dispatcher で該当 handler にだけ渡す

```rust
pub(crate) async fn run(
    effect: Effect,
    action_tx: &mpsc::Sender<Action>,
    state: &mut AppState,
    completion_engine: &RefCell<...>,
) -> Result<()>
```

## RefCell borrow 安全ルール

`completion_engine` は `RefCell` なので共通引数にバンドルせず、必要な handler のみ引数で受ける。
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

0. **I/O を伴う場合は `app/ports/` に port trait を定義し、`infra/adapters/` で実装すること（app 層の I/O 禁止ルール）**
1. `effect.rs` に variant 追加
2. 対応する `effect_handlers/<feature>.rs` の match arm に追加
3. `effect_runner.rs` の dispatcher match arm に追加（既存の `e @` パターンに追記）
4. handler のテストを追加
