---
paths:
  - "**/tests/render_snapshots.rs"
  - "**/tests/snapshots/**"
---

# Visual Regression Testing

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
└── snapshots/           # Generated .snap files (auto-created by insta)
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

## Coverage Criteria

### Mode Coverage Obligation (MUST)

- Every `InputMode` variant MUST have at least one snapshot test
- When adding a new `InputMode`, add a corresponding snapshot BEFORE the PR is merged

### When to Add a Snapshot Test

- **Each InputMode** - At least one scenario per mode
- **Major UI state changes** - Focus pane switching, message display
- **Boundary conditions** - Empty results, initial loading state, error states
- **Text input components** - Cursor at head, middle, and tail positions (3 states minimum)

### When NOT to Add

- **Data variations** - Different row counts, column counts within same screen
- **Exhaustive combinations** - All possible state permutations
- **Transient states** - Brief loading indicators (except persistent ones like ER progress)

## Snapshot Update Policy

### Allowed

- **Intentional UI changes** - Layout, styling, new features
- **Bug fixes that change visual output** - After fixing the display bug

### Not Allowed

- **Failing tests due to regressions** - Fix the code, not the snapshot
- **Unintentional changes** - Investigate the diff first
