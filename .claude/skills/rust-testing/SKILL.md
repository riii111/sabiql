---
name: rust-testing
description: Write Rust tests following project conventions. Relevant when writing tests, adding test cases, or improving coverage for unit/integration testing.
---

# Rust Testing Skill

## Guidelines

Follow the conventions defined in:
- `.claude/rules/rust-testing-style.md` — naming, structure, rstest vs `#[test]`
- `.claude/rules/testing-obligations.md` — what MUST be tested per layer

## Quick Reference

1. Check which layer the code belongs to (domain/app/infra/ui)
2. Consult the obligations table for MUST-test scenarios
3. Use the style guide for naming and structure
4. For snapshot tests, see `.claude/rules/visual-regression.md`
