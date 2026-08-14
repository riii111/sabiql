# Rust style

- Follow Newspaper style: place public APIs and high-level processing before internal helpers.
- Keep `#[cfg(test)]`-only items inside a test module. `#[cfg(test)] use` and shared `feature = "test-support"` items are exceptions to this rule.
