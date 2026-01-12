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
| Data definitions | `app/keybindings.rs` | Single source of truth for key/description/Action |
| Display logic | `ui/components/footer.rs` | Context-sensitive hint selection by InputMode/state |
| Full reference | `ui/components/help_overlay.rs` | Complete keybinding reference |
| Command list | `app/palette.rs` | Commands shown in Command Palette |

When adding a new keybinding:
1. Add data to `keybindings.rs`
2. Implement event handler in `handler.rs`
3. Update Footer/Help/Palette as needed
