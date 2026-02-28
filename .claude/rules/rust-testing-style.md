---
paths:
  - "**/src/**/*.rs"
  - "**/tests/**/*.rs"
---

# Rust Testing Style Guidelines

## Basic Principles

- Use standard `#[test]` + `rstest`
- Use `mockall` for mocking (only when necessary)
- Use `#[tokio::test]` for async tests

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

## rstest Cohesion Rules

Before adding cases to an existing `#[rstest]` function, verify all cases belong to the same **behavior category**.

- If multiple categories are mixed, split by category:
  - Examples: by `ErrorKind` / valid-invalid / key role
- vim/arrow alias pairs belong in the **same** function (do not split them)
- **Guideline**: if a function exceeds 8 cases, re-evaluate whether it needs splitting

```rust
// ✅ Each function tests a single behavior category
#[rstest]
#[case("psql: command not found")]
#[case("not found: mysql")]
fn classify_stderr_as_cli_not_found(#[case] stderr: &str) { ... }

#[rstest]
#[case("Connection refused")]
#[case("Some random error")]
#[case("")]
fn classify_stderr_as_unknown_fallback(#[case] stderr: &str) { ... }

// ✅ vim/arrow aliases stay together
#[rstest]
#[case(Key::Up, Action::ScrollUp)]
#[case(Key::Char('k'), Action::ScrollUp)]
#[case(Key::Down, Action::ScrollDown)]
#[case(Key::Char('j'), Action::ScrollDown)]
fn scroll_keys(#[case] code: Key, #[case] expected: Action) { ... }
```

## Test mod Structure Rules

- When `#[test]` functions in a single file exceed **20**, group them into `mod` blocks
- `mod` names should be nouns describing the behavior domain (e.g., `connection_error`, `result_pane`)
- Flat test lists make it unclear "where to look for what"

```rust
// ✅ grouped by behavior domain
mod connection_flow { ... }
mod overlays { ... }
mod result_pane { ... }
```

## Fixture Extraction Rules

- If the same struct literal appears in **2 or more** tests, extract it as a helper function
- If an inline struct exceeds **15 lines**, consider extracting it as a fixture
- If the same helper function is defined in **2 sub-mods**, move it to the parent `mod tests` scope

```rust
// ✅ extracted to parent mod tests scope
fn create_test_profile(name: &str) -> ConnectionProfile { ... }
fn minimal_users_table() -> Table { ... }
fn create_table(schema: &str, name: &str, columns: &[&str]) -> Table { ... }
```

## Summary

| Test Type | Style |
| --------- | ----- |
| VO / Simple logic | `#[test]` flat |
| Boundary values / Patterns | `rstest` + `#[case]` |
| Feature groups | Nested with `mod` |
| Integration / E2E | `tests/` directory |
| rstest with mixed categories | Split by behavior category |
| Repeated struct literals | Extract as helper function |
