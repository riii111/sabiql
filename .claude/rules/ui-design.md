---
description: UI design rules for sabiql - Atomic Design pattern, footer hints, keybindings
alwaysApply: false
globs:
  - "**/src/ui/**/*.rs"
---

# UI Design Rules

## Component Structure (Atomic Design)

UI components follow the Atomic Design pattern:

```
src/ui/components/
├── atoms/       # Smallest reusable units (spinner, key_chip, panel_border)
├── molecules/   # Compositions of atoms (modal_frame, hint_bar)
└── *.rs         # Organisms: screen-level components (explorer, inspector, etc.)
```

| Layer | Purpose | Examples |
|-------|---------|----------|
| atoms | Single-purpose primitives | `spinner_char()`, `key_chip()`, `panel_block()` |
| molecules | Reusable patterns combining atoms | `render_modal()`, `hint_line()` |
| organisms | Screen sections, may use molecules/atoms | `Explorer`, `SqlModal`, `Footer` |

When adding UI components:
- Extract repeated visual patterns into atoms/molecules
- Use `Theme::*` tokens instead of raw `Color::*` values
- Organisms should compose molecules/atoms, not duplicate their logic

## Footer Hint Ordering

All input modes must follow this ordering:

```
Actions → Navigation → Help → Close/Cancel → Quit
```

## Keybindings & Commands

Keybinding and command definitions follow this architecture:

| Concept | Location | Responsibility |
|---------|----------|----------------|
| Data definitions | `app/keybindings/` | SSOT module: `KeyBinding` (simple modes) + `ModeRow`/`ModeBindings` (mixed modes). Submodules: `normal.rs`, `overlays.rs`, `connections.rs`, `editors.rs`, `types.rs` |
| Key resolution | `app/keymap.rs` | `resolve(combo, bindings)` for `KeyBinding` slices; `resolve_mode(combo, rows)` for `ModeRow` slices — both called via `ModeBindings::resolve()` or directly |
| Key translation | `ui/event/key_translator.rs` | `translate(KeyEvent) -> KeyCombo` — crossterm adapter |
| Mode dispatch | `ui/event/handler.rs` | Routes `KeyCombo` to handler by `InputMode` |
| Display logic | `ui/components/footer.rs` | Context-sensitive hint selection by InputMode/state |
| Full reference | `ui/components/help_overlay.rs` | Complete keybinding reference |
| Command list | `app/palette.rs` | Commands shown in Command Palette |

When adding a new keybinding:
1. Add an entry to the appropriate submodule in `keybindings/` (`normal.rs`, `overlays.rs`, `connections.rs`, `editors.rs`)
2. **Modes with `ModeBindings`** (Help, ConnectionError, ConnectionSelector, CommandPalette, TablePicker, ErTablePicker): add a `ModeRow` entry to the corresponding `*_ROWS` constant with display text and `bindings: &[ExecBinding { ... }]`. `ModeBindings::resolve()` handles dispatch automatically.
3. **Normal mode**: also add a predicate function (e.g., `pub fn is_foo(combo: &KeyCombo) -> bool`) in `mod.rs` and wire it in `handle_normal_mode` in `handler.rs`
4. Update Footer/Help/Palette display as needed

**Char fallback rule**: Modes with freeform text input (TablePicker, ErTablePicker, CommandLine, CellEdit) use `keymap::resolve()` first, then fall through to `Char(c)` for text input. Do NOT add plain `KeyCombo::plain(Key::Char(x))` combos to these modes for command keys — use non-Char keys (Up/Down/Esc/Enter) instead.
