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

### ユニットテスト（モジュール内）

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_email_returns_ok() {
        let input = "user@example.com";

        let result = Email::new(input);

        assert!(result.is_ok());
    }
}
```

### インテグレーションテスト

`tests/` ディレクトリに配置。

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

## まとめ

| テスト種別 | スタイル |
|-----------|---------|
| VO / 単純ロジック | `#[test]` フラット |
| 境界値 / パターン | `rstest` + `#[case]` |
| 機能グループ | `mod` でネスト |
| インテグレーション / E2E | `tests/` ディレクトリ |
