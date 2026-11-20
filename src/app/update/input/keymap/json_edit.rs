use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, CursorMove, InputTarget};

use super::common::consume_readline;
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface, KeyContext};

fn supported(context: &KeyContext) -> bool {
    context.supports_json_edit()
}

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let target = InputTarget::JsonEdit;
    let mut rows = vec![Row {
        bindings: vec![
            Binding::no_ctrl_alt(Action::JsonExitEdit, Key::Esc).when(supported),
            Binding::action(Action::JsonExitEdit, &[KeyCombo::plain(Key::Esc)]),
        ],
        short: "Normal",
        description: "Return to Normal mode",
        references: &[],
        dynamic_label: None,
        visible: None,
        surfaces: &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    }];
    rows.extend(consume_readline(target));
    rows.push(Row {
        bindings: [
            (Key::Left, CursorMove::Left),
            (Key::Right, CursorMove::Right),
            (Key::Up, CursorMove::Up),
            (Key::Down, CursorMove::Down),
        ]
        .into_iter()
        .map(|(key, direction)| {
            Binding::any_modifiers(Action::TextMoveCursor { target, direction }, key)
        })
        .collect(),
        short: "Move",
        description: "Move cursor",
        references: &[],
        dynamic_label: None,
        visible: None,
        surfaces: &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    });
    rows.push(Row {
        bindings: [(Key::Home, CursorMove::Home), (Key::End, CursorMove::End)]
            .into_iter()
            .map(|(key, direction)| {
                Binding::any_modifiers(Action::TextMoveCursor { target, direction }, key)
            })
            .collect(),
        short: "Line",
        description: "Line start/end",
        references: &[],
        dynamic_label: None,
        visible: None,
        surfaces: &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    });
    for (key, action) in [
        (Key::Backspace, Action::TextBackspace { target }),
        (Key::Delete, Action::TextDelete { target }),
        (Key::Enter, Action::TextInput { target, ch: '\n' }),
        (Key::Tab, Action::TextInput { target, ch: '\t' }),
    ] {
        rows.push(Row {
            bindings: vec![Binding::any_modifiers(action, key)],
            short: "",
            description: "",
            references: &[],
            dynamic_label: None,
            visible: None,
            surfaces: &[],
        });
    }
    rows.push(Row {
        bindings: vec![Binding::text_any_modifiers(target)],
        short: "",
        description: "",
        references: &[],
        dynamic_label: None,
        visible: None,
        surfaces: &[],
    });
    rows
});

pub fn json_edit_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn json_edit_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
