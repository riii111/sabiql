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

## Coverage Criteria

### When to Add a Snapshot Test

- **Each InputMode** - At least one scenario per mode
- **Major UI state changes** - Focus pane switching, focus mode toggle, message display
- **Boundary conditions** - Empty results, initial loading state, error states

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
