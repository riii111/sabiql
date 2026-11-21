use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, CursorMove, InputTarget};

use super::common::consume_readline;
use super::model::{Binding, Row};
use super::{HintSurface, KeyContext};

fn supported(context: &KeyContext) -> bool {
    context.supports_json_edit()
}

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let target = InputTarget::JsonEdit;
    let mut rows = vec![Row::new(
        vec![
            Binding::no_ctrl_alt(Action::JsonExitEdit, Key::Esc).when(supported),
            Binding::action(Action::JsonExitEdit, &[KeyCombo::plain(Key::Esc)]),
        ],
        "Normal",
        "Return to Normal mode",
        &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    )];
    rows.extend(consume_readline(target));
    rows.push(Row::new(
        [
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
        "Move",
        "Move cursor",
        &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    ));
    rows.push(Row::new(
        [(Key::Home, CursorMove::Home), (Key::End, CursorMove::End)]
            .into_iter()
            .map(|(key, direction)| {
                Binding::any_modifiers(Action::TextMoveCursor { target, direction }, key)
            })
            .collect(),
        "Line",
        "Line start/end",
        &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    ));
    for (key, action) in [
        (Key::Backspace, Action::TextBackspace { target }),
        (Key::Delete, Action::TextDelete { target }),
        (Key::Enter, Action::TextInput { target, ch: '\n' }),
        (Key::Tab, Action::TextInput { target, ch: '\t' }),
    ] {
        rows.push(Row::new(
            vec![Binding::any_modifiers(action, key)],
            "",
            "",
            &[],
        ));
    }
    rows.push(Row::new(
        vec![Binding::text_any_modifiers(target)],
        "",
        "",
        &[],
    ));
    rows
});
