---
description: Rust testing style guidelines - test structure, naming conventions, rstest usage
alwaysApply: false
globs:
  - "**/src/**/*.rs"
  - "**/tests/**/*.rs"
---

# Rust Testing Style Guidelines

## Basic Principles

- Use standard `#[test]` + `rstest`
- Use `mockall` for mocking (only when necessary)
- Use `#[tokio::test]` for async tests

## Testing Targets by Layer

| Layer | Testing Target | Location |
| ----- | -------------- | -------- |
| **Domain** | Value validation, invariants | `#[cfg(test)]` in `src/domain/*.rs` |
| **App** | State transitions, action processing | `#[cfg(test)]` in `src/app/*.rs` |
| **Infra** | CLI output parsing, cache behavior | `#[cfg(test)]` in `src/infra/*.rs` |
| **UI** | Component rendering logic | `#[cfg(test)]` in `src/ui/*.rs` |
| **Integration** | Multi-layer integration | `tests/*.rs` |

## Test Structure

### Unit Tests (within modules)

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

### Boundary Value & Pattern Tests (rstest)

```rust
use rstest::rstest;

#[rstest]
#[case("Aa1!aaa", false)]    // 7 characters
#[case("Aa1!aaaa", true)]    // 8 characters
fn password_length_validation(#[case] input: &str, #[case] expected: bool) {
    let result = Password::new(input);
    assert_eq!(result.is_ok(), expected);
}
```

### When to Use rstest vs #[test]

Use `rstest` when:
- The test is a pure mapping of input → expected output with many similar cases.
- There are multiple aliases, key variants, or boundary patterns that fit a table.

Use individual `#[test]` when:
- The case is a spec-critical behavior that should be easy to read by name.
- The behavior is a special branch or regression scenario that deserves a dedicated test.

### Grouping (using mod for context)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod save {
        use super::*;

        #[test]
        fn inserts_new_user() { ... }

        #[test]
        fn returns_duplicate_error_on_conflict() { ... }
    }
}
```

## Naming Conventions

### Function Name Patterns

- `<condition>_returns_<result>`
- `<condition>_<action>s_<result>`

Examples:
- `valid_input_returns_ok`
- `empty_string_returns_validation_error`
- `duplicate_email_returns_conflict`

## Test Structure (given/when/then)

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

## Summary

| Test Type | Style |
| --------- | ----- |
| VO / Simple logic | `#[test]` flat |
| Boundary values / Patterns | `rstest` + `#[case]` |
| Feature groups | Nested with `mod` |
| Integration / E2E | `tests/` directory |
