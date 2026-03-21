---
paths:
  - "**/src/**/*.rs"
  - "**/tests/**/*.rs"
---

# テストルール

## 原則

テストは振る舞いを文書化する。テスト名だけで「何が壊れたか」がわかるようにする。
理由: CI の失敗ログでテスト名しか見えない状況で、名前が意味を持たないと調査コストが跳ね上がる。

- モックが必要な場合は `mockall` を使う
- 非同期テストは `#[tokio::test]` を使う

## 命名

- `<条件>_returns_<結果>` または `<条件>_<動作>s_<結果>`
- 例: `valid_input_returns_ok`, `empty_string_returns_validation_error`

## 構造

given/when/then を空行で表現する（コメントではなく行間で区切る）。

## テスト種別の選択

| 状況 | 種別 |
|------|------|
| VO・単純ロジック | `#[test]` フラット |
| 入力→期待出力の純粋マッピング、類似ケースが多い | `rstest` + `#[case]` |
| 仕様上重要な振る舞い、リグレッション | 個別 `#[test]`（名前で意図を伝える） |
| 1ファイル内のテストが20個超 | `mod` でグループ化（名前は振る舞いドメイン名詞） |
| レイヤ横断ラウンドトリップ | `tests/` ディレクトリ |

## rstest 凝集度

- 1つの `#[rstest]` 関数は単一の振る舞いカテゴリのみテストすること
- 複数カテゴリが混在している場合はカテゴリごとに分割する
- vim/矢印キーのエイリアスペアは同じ関数内に置く
- 8ケース超で分割を検討する

```rust
// vim/矢印エイリアスはまとめてよい
#[rstest]
#[case(Key::Up, Action::ScrollUp)]
#[case(Key::Char('k'), Action::ScrollUp)]
#[case(Key::Down, Action::ScrollDown)]
#[case(Key::Char('j'), Action::ScrollDown)]
fn scroll_keys(#[case] code: Key, #[case] expected: Action) { ... }
```

## fixture 抽出

- 同じ構造体リテラルが2つ以上のテストに出現 → ヘルパー関数に抽出
- 同じヘルパーが2つの子 mod で定義 → 親の `mod tests` スコープに移動

## レイヤ別カバレッジ義務

| レイヤ | 必須テスト対象 |
|--------|---------------|
| Domain | public コンストラクタ・バリデーション |
| App (reducers) | 状態遷移（Action → state diff） |
| App (ports) | port trait のデフォルト実装メソッド |
| Infra (parsers) | CLI 出力パース |
| Infra (adapters) | SQL 生成（方言固有） |
| UI | 描画の境界条件（空テーブル、オーバーフロー、エラー） |
| Integration | `tests/render_snapshots/<category>.rs` |

## `#[ignore]` トラッキング

すべての `#[ignore]` にトラッキング Issue リンクが必要:
```rust
#[ignore] // tracked: #42 — MySQL adapter 待ち
```
