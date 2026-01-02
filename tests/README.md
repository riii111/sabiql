# Snapshot Test Policy

## When Snapshots CAN Be Updated

- **Intentional UI changes** - Layout, styling, new features
- **Bug fixes that change visual output** - After fixing the display bug

## When Snapshots MUST NOT Be Updated

- **Failing tests due to regressions** - Fix the code, not the snapshot
- **Unintentional changes** - Investigate the diff first

## Commands

```bash
mise run test                      # Run all tests
mise exec -- cargo insta review    # Review pending snapshots interactively
mise exec -- cargo insta accept    # Accept all pending snapshots
mise exec -- cargo insta reject    # Reject all pending snapshots
```

## Adding New Scenarios

1. Add test in `tests/render_snapshots.rs`
2. Run `mise run test` (will fail with new snapshot)
3. Review the `.snap.new` file
4. Run `mise exec -- cargo insta accept`
