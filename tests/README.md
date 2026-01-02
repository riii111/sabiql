# Visual Regression Testing

This project uses snapshot testing to detect unintended UI changes.

## Overview

- **Library**: [insta](https://insta.rs) - Rust snapshot testing
- **Scope**: Tests `AppState` → `MainLayout::render()` integration
- **Backend**: Ratatui `TestBackend` (in-memory terminal 80x24)

## Directory Structure

```
tests/
├── harness/
│   ├── mod.rs       # Test utilities (render_to_string, create_test_*)
│   └── fixtures.rs  # Sample data builders (metadata, table detail, query result)
├── render_snapshots.rs  # Snapshot test scenarios
├── snapshots/           # Generated .snap files (auto-created by insta)
└── README.md
```

## Commands

```bash
mise run test                      # Run all tests
mise exec -- cargo insta review    # Review pending snapshots interactively
mise exec -- cargo insta accept    # Accept all pending snapshots
mise exec -- cargo insta reject    # Reject all pending snapshots
```

## Adding New Scenarios

1. Add test function in `tests/render_snapshots.rs`
2. Run `mise run test` (will fail with new snapshot)
3. Review the generated `.snap.new` file
4. Run `mise exec -- cargo insta accept`

## Snapshot Update Policy

### Allowed

- **Intentional UI changes** - Layout, styling, new features
- **Bug fixes that change visual output** - After fixing the display bug

### Not Allowed

- **Failing tests due to regressions** - Fix the code, not the snapshot
- **Unintentional changes** - Investigate the diff first

## Test Harness

The `harness` module provides utilities for deterministic rendering:

| Function | Purpose |
|----------|---------|
| `create_test_state()` | Creates `AppState` with test defaults |
| `create_test_terminal()` | Creates 80x24 `TestBackend` terminal |
| `render_to_string()` | Renders state to string with fixed time (0ms) |
| `fixed_instant()` | Returns consistent `Instant` for message expiry |

Time-dependent elements (spinner, message expiry) use injected values for reproducibility.
