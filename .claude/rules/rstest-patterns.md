---
paths:
  - "**/src/domain/**/*.rs"
  - "**/src/infra/adapters/**/parser*.rs"
  - "**/src/infra/adapters/**/parser/**/*.rs"
  - "**/src/infra/adapters/**/sql/**/*.rs"
---

# rstest パターンガイド

## rstest の凝集度ルール

既存の `#[rstest]` 関数にケースを追加する前に、すべてのケースが同じ**振る舞いカテゴリ**に属しているか確認すること。

- 複数カテゴリが混在している場合はカテゴリごとに分割する
  - 例: `ErrorKind` 別 / valid-invalid 別 / キーの役割別
- vim/矢印キーのエイリアスペアは**同じ関数内**に置く（分割しない）
- **目安**: 1関数が8ケースを超えたら分割を検討する

```rust
// ✅ 各関数が単一の振る舞いカテゴリをテスト
#[rstest]
#[case("psql: command not found")]
#[case("not found: mysql")]
fn classify_stderr_as_cli_not_found(#[case] stderr: &str) { ... }

#[rstest]
#[case("Connection refused")]
#[case("Some random error")]
#[case("")]
fn classify_stderr_as_unknown_fallback(#[case] stderr: &str) { ... }

// ✅ vim/矢印エイリアスはまとめて良い
#[rstest]
#[case(Key::Up, Action::ScrollUp)]
#[case(Key::Char('k'), Action::ScrollUp)]
#[case(Key::Down, Action::ScrollDown)]
#[case(Key::Char('j'), Action::ScrollDown)]
fn scroll_keys(#[case] code: Key, #[case] expected: Action) { ... }
```

## rstest を使うべきとき

- テストが入力→期待出力の純粋なマッピングで、類似ケースが多い場合
- 複数のエイリアス、キーバリアント、境界値パターンがテーブルに収まる場合

## 個別 #[test] を使うべきとき

- 仕様上重要な振る舞いで、テスト名から読み取れることが大事な場合
- 特殊な分岐やリグレッションシナリオで、専用テストに値する場合
