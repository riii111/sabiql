---
paths:
  - "**/src/**/*.rs"
  - "**/tests/**/*.rs"
---

# Rust テストスタイルガイド

## 基本原則

- 標準の `#[test]` + `rstest` を使う
- モック必要時は `mockall` を使う
- 非同期テストには `#[tokio::test]` を使う

## テスト構造

- ユニットテストは `#[cfg(test)] mod tests` でモジュール内に配置
- インテグレーションテストは `tests/` ディレクトリに配置

## 命名規約

- `<条件>_returns_<結果>`
- `<条件>_<動作>s_<結果>`

例:
- `valid_input_returns_ok`
- `empty_string_returns_validation_error`
- `duplicate_email_returns_conflict`

## テスト構造（given/when/then）

```rust
#[test]
fn register_with_valid_input_returns_registered_user() {
    // given
    let command = RegisterCommand { ... };
    let mock_repo = MockRepository::new();

    // when
    let result = use_case.register(command, &mock_repo);

    // then
    assert!(result.is_ok());
}
```
（実際にgiven, when, thenをコメントで書くのではなく、行間で表現する)

## まとめ

| テスト種別 | スタイル |
|-----------|---------|
| VO / 単純ロジック | `#[test]` フラット |
| 境界値 / パターン | `rstest` + `#[case]` |
| 機能グループ | `mod` でネスト |
| インテグレーション / E2E | `tests/` ディレクトリ |
