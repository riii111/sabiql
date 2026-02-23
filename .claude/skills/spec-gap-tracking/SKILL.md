---
name: spec-gap-tracking
description: Audit #[ignore] tests, TODO/FIXME comments, and specification gaps. Relevant when reviewing test coverage, discussing technical debt, or tracking deferred features.
user-invocable: false
---

# Specification Gap Tracking

## When to Use

- Periodic audit of test health
- Before major releases
- When user asks "what tests are missing?" or "what's deferred?"

## Procedure

1. Search for `#[ignore]` across all `.rs` files
   - Each MUST have a tracking comment `// tracked: #<issue> — <reason>`
   - Report any bare `#[ignore]` as violations
2. Search for `TODO`, `FIXME`, `HACK`, `XXX` comments
   - Categorize by layer (domain/app/infra/ui)
   - Check if linked to an Issue
3. Cross-reference `InputMode` variants against snapshot tests
   - Report any mode without a snapshot test
4. Check testing-obligations coverage table against actual test files

## Output

- Table of gaps: location, type (ignore/TODO/missing-test), linked issue, status
- Summary counts by category

## Exit Criteria

- All `#[ignore]` have tracking comments
- All gaps are documented with Issue links

## Escalation

- If >5 untracked gaps found, propose a dedicated cleanup Issue
