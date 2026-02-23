---
paths:
  - "**/src/app/keybindings/**/*.rs"
  - "**/src/ui/event/**/*.rs"
  - "**/src/ui/components/footer.rs"
  - "**/src/ui/components/help_overlay.rs"
  - "**/src/app/palette.rs"
---

# Interaction Contract

## Single Source of Truth (MUST)

- `app/keybindings/` is the SSOT for ALL key bindings
- Footer hints, Help overlay, and Command Palette MUST derive from keybindings data
- NEVER define a key combo in `handler.rs` that is not declared in `keybindings/`

## Consistency Invariants (MUST)

1. Every `KeyBinding` / `ModeRow` entry with a display label MUST appear in Help overlay
2. Every keybinding shown in Footer MUST resolve to an action in `handler.rs`
3. Command Palette entries MUST map to the same action names as keybindings

## Adding a New Keybinding — Full Checklist

1. Add entry in `app/keybindings/{normal,overlays,connections,editors}.rs`
2. If Normal mode: add predicate fn in `keybindings/mod.rs` + wire in `handler.rs`
3. If ModeBindings mode: add `ModeRow` entry; dispatch is automatic
4. Update Footer `display_hint` if the binding should be visible
5. Update Help overlay section for the relevant mode
6. If the action is palette-worthy: add to `app/palette.rs`
7. Run snapshot tests to verify footer/help rendering

## Anti-patterns (FORBIDDEN)

- Hardcoded key checks in `handler.rs` without `keybindings/` entry
- Footer hint text that does not match keybindings display label
- Help overlay listing a key that has no corresponding keybinding entry
