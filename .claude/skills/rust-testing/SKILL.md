---
name: rust-testing
description: >
  Write Rust tests following project conventions. Auto-fires when: user asks
  to write tests, add test cases, improve coverage, or mentions unit/integration
  testing. Covers test structure, naming, rstest usage, layer-specific test
  targets, and testing obligations.
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
