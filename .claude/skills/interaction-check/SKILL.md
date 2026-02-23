---
name: interaction-check
description: Verify keybinding consistency across SSOT, handler, footer, help overlay, and command palette. Relevant when adding/modifying keybindings or InputMode handling.
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
