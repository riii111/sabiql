---
paths:
  - "**/src/app/**/*.rs"
  - "**/src/ui/event/**/*.rs"
---

# テスト構成ルール

## mod 構造ルール

- 1ファイル内の `#[test]` 関数が **20個を超えたら**、`mod` ブロックでグループ化する
- `mod` 名は振る舞いドメインを表す名詞にする（例: `connection_error`, `result_pane`）
- フラットなテストリストは「どこに何があるか」が分かりにくくなる

```rust
// ✅ 振る舞いドメインでグループ化
mod connection_flow { ... }
mod overlays { ... }
mod result_pane { ... }
```

## フィクスチャ抽出ルール

- 同じ構造体リテラルが **2つ以上** のテストに出現したら、ヘルパー関数に抽出する
- インライン構造体が **15行を超えたら**、フィクスチャとして抽出を検討する
- 同じヘルパー関数が **2つの子 mod** で定義されていたら、親の `mod tests` スコープに移動する

```rust
// ✅ 親 mod tests スコープに抽出
fn create_test_profile(name: &str) -> ConnectionProfile { ... }
fn minimal_users_table() -> Table { ... }
fn create_table(schema: &str, name: &str, columns: &[&str]) -> Table { ... }
```
