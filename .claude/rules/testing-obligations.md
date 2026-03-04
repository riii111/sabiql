---
paths:
  - "**/src/**/*.rs"
  - "**/tests/**/*.rs"
---

# テスト義務

## レイヤ別の必須テスト対象

| レイヤ | 必須テストシナリオ | 例 |
|--------|-------------------|-----|
| **Domain** | すべての public コンストラクタ / バリデーション | `ConnectionConfig::new()` の境界値 |
| **App (reducers)** | `AppState` を変更するすべての状態遷移 | Action ディスパッチ → state diff |
| **App (ports)** | port trait のデフォルト実装メソッド | `DdlGenerator::ddl_line_count()` |
| **Infra (parsers)** | 各 adapter の CLI 出力パース | `psql` テーブル出力 → `QueryResult` |
| **Infra (adapters)** | SQL 生成（方言固有） | PG 用 `build_update_sql`（MySQL 実装時はそちらも） |
| **UI (components)** | 描画の境界条件 | 空テーブル、オーバーフロー、エラー状態 |
| **Integration** | レイヤ横断のラウンドトリップ | `tests/render_snapshots.rs` |

## `#[ignore]` トラッキングルール（必須）

- すべての `#[ignore]` テストにトラッキング Issue へのリンクコメントが必要
- 形式: `#[ignore] // tracked: #<issue番号> — <理由>`
- リンク先 Issue を解決したら `#[ignore]` を削除または更新すること

```rust
#[ignore] // tracked: #42 — MySQL adapter 待ち
#[test]
fn mysql_query_parsing() { ... }
```

## スナップショットテスト義務

- 新しい `InputMode` バリアントには `tests/render_snapshots.rs` に最低1つのスナップショットが必要
- 詳細なカバレッジ基準は `visual-regression.md` を参照

## PR セルフチェック（Claude 向け）

PR を ready にする前に:
- [ ] 新しい public 関数にユニットテストがある
- [ ] adapter の SQL 生成がサポート対象の全方言でテストされている
- [ ] トラッキング Issue なしの `#[ignore]` がない
- [ ] 新しい `InputMode` バリアントにスナップショットテストがある
