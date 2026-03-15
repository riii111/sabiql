---
paths:
  - "**/src/app/state.rs"
  - "**/src/app/browse_session.rs"
  - "**/src/app/result_interaction.rs"
  - "**/src/app/services.rs"
  - "**/src/app/reducer.rs"
  - "**/src/app/reducers/**/*.rs"
  - "**/src/app/query_execution.rs"
  - "**/src/ui/shell/footer.rs"
  - "**/src/ui/features/browse/result.rs"
  - "**/src/ui/event/**/*.rs"
---

# AppState 不変条件

## 派生状態パターン

`AppState` のフィールドが他フィールドから派生する場合（例: `connection_list_items` は `connections` + `service_entries` から派生）、**ソースフィールドはすべて private** にし、派生フィールドを自動再構築する setter 経由でのみ変更すること。

| パターン | 可否 |
|---------|------|
| 派生グループへの直接代入（`state.foo = x`） | **禁止** |
| 自動再構築付き setter（`state.set_foo(x)`） | **必須** |
| `rebuild_*()` を公開APIとして提供 | **禁止**（private にして setter 内部から呼ぶ） |

### 既存の適用例

- **Connection グループ**: `connections`, `service_entries` → `connection_list_items`
  - Setter: `set_connections`, `set_service_entries`, `set_connections_and_services`, `retain_connections`
  - Getter: `connections()`, `service_entries()`, `connection_list_items()`

新しい派生フィールドを追加する場合も同じパターンを適用すること。

## ResultInteraction aggregate

`ResultInteraction`（`app/result_interaction.rs`）は Result pane のインタラクション状態を集約する。

### Ownership

`selection`, `cell_edit`, `staged_delete_rows`, `pending_write_preview` は private。遷移時に同時 reset が必要で、バラバラに触ると不整合が起きるため。

### 不変条件

- co-dependent fields の同時リセットは aggregate の transition boundary を通すこと。reducer が private fields の個別 clear を組み合わせてはならない
- 新しい Result interaction state を追加したら、既存の transition メソッドへの統合を検討すること
- `input_mode` の caller 責務: modal 遷移を伴う transition の後、caller が `input_mode` を適切に戻すこと（SAB-136 で統合予定）

## BrowseSession aggregate

`BrowseSession`（`app/browse_session.rs`）は接続・メタデータ・テーブル選択のライフサイクルを集約する。

### Co-dependent fields（private）

| グループ | フィールド | 遷移 API |
|---------|-----------|----------|
| 接続ライフサイクル | `connection_state`, `metadata_state` | `begin_connecting`, `mark_connected`, `mark_connection_failed` |
| テーブル選択 | `current_table`, `table_detail`, `selection_generation` | `select_table`, `set_table_detail`, `clear_table_selection` |
| ライフサイクル制約 | `metadata` | `mark_connected`, `restore_from_cache`, `reset` |

### 不変条件

- `connection_state` と `metadata_state` は常にペアで遷移すること。reducer が個別に `set_connection_state` / `set_metadata_state` を組み合わせてはならない（transitional raw setter は Phase 3 で除去予定）
- テーブル選択の変更は `select_table` / `clear_table_selection` を通すこと。`selection_generation` は非同期結果の stale check に使われるため、選択解除でも bump が必要
- `database_name` は `metadata` から導出される（single source of truth）。別フィールドとして持たない
- `reset` / `restore_from_cache` は aggregate boundary を通すこと。reducer が raw setter を並べて手書き reset してはならない
- `restore_from_cache` は `selection_generation = 0`, `is_reloading = false` にリセットして stale token 境界を閉じる

## pub フィールドの型設計

- `pub` フィールドに **3 要素以上の匿名タプル** を使わないこと。名前付き構造体を定義する
- 2 要素でも、destructure した変数名なしでは意味が読み取れない場合は構造体にする
- 理由: 展開先で位置ベースの destructure（`(_, _, until)`）が必要になり、意味が不透明になる。派生値で `Option<Option<T>>` のような読みづらい型が生まれやすい

| パターン | 可否 |
|---------|------|
| `pub flash: Option<(usize, Option<usize>, Instant)>` | **禁止** — 3要素+ネスト、意味不明 |
| `pub flash: Option<YankFlash>`（named struct） | **推奨** |
| `pub pos: Option<(usize, usize)>`（row, col） | 許容 — 座標ペアは慣習的に自明 |
| `pub pair: Option<(String, Instant)>` | 微妙 — フィールド名で補えるなら許容、迷ったら構造体 |

## State/View 分離

- カーソル位置をコンテンツ `String` の一部としてエンコードしてはならない（例: テキスト中にカーソル文字を挿入する）
- カーソル位置は View 層の関心事であり、State 内では独立した数値インデックスとして保持すること

## 状態と依存の分離

`AppState` は純粋な状態のみを保持する。Port trait 実装やチャネルなどの依存オブジェクトは `AppServices` に格納し、reducer / renderer には引数で注入する。

| 分類 | 配置先 | 例 |
|------|--------|-----|
| 純粋な状態 | `AppState` | `ui`, `session`, `query`, `connections` |
| Port trait 実装 | `AppServices` | `DdlGenerator`, `SqlDialect` |
| I/O 用 Port | `EffectRunner` | `MetadataProvider`, `QueryExecutor` |

- `AppState` に `Arc<dyn Trait>`, `Sender`, `Rc<RefCell<...>>` 等の依存を追加してはならない
- Reducer sig: `reduce(state: &mut AppState, action, now, services: &AppServices)`
- Renderer sig: `draw(state: &mut AppState, services: &AppServices)`

## Visible Result read model

Result pane の「表示中 result」を `current_result` / `history_index` / `result_history` の組み合わせで各所が再解釈してはならない。`QueryExecution` が提供する read model と capability API を使うこと:

| API | 用途 |
|-----|------|
| `visible_result()` | 表示中の `&QueryResult` を取得 |
| `visible_result_kind()` | 表示中 result の意味分類（`VisibleResultKind`） |
| `is_history_mode()` | history 閲覧中かどうか |
| `can_edit_visible_result()` | 編集可能かどうか（LivePreview のみ） |
| `can_paginate_visible_result()` | ページネーション可能かどうか（LivePreview のみ） |
| `history_bar()` | history バー表示用 `(index, total)` |
| `has_history_hint()` | history ヒント表示判定 |

- Reducer による `history_index` の直接変更は許可（write は所有者、read は API 経由）
