---
name: interaction-check
description: >
  Verify keybinding consistency across SSOT (app/keybindings), handler dispatch,
  Footer hints, Help overlay, and Command Palette. Auto-fires when: adding or
  modifying keybindings, changing InputMode handling, updating footer or help
  overlay text, reviewing PRs that touch event handling or UI hints.
  Does NOT fire for: pure styling changes, non-UI refactors.
user-invocable: false
---

# Interaction Consistency Check

## When to Use

- After adding/modifying keybindings
- During PR review of event handling or UI hint changes
- When a user reports "key does nothing" or "help shows wrong key"

## Procedure

1. List all entries in `app/keybindings/{normal,overlays,connections,editors}.rs`
2. For each entry with a display label:
   a. Verify it appears in `ui/components/help_overlay.rs`
   b. Verify the handler in `ui/event/handler.rs` dispatches it
   c. If visible in footer, verify `ui/components/footer.rs` shows matching text
3. For Command Palette entries in `app/palette.rs`:
   a. Verify each maps to a declared keybinding action
4. Report any orphaned or inconsistent entries

## Output

- List of inconsistencies (file, line, description)
- Suggested fixes

## Exit Criteria

- Zero inconsistencies found, OR all inconsistencies have fix suggestions

## Escalation

- If a structural pattern makes consistency hard to maintain, propose a rules update to `interaction-contract.md`
