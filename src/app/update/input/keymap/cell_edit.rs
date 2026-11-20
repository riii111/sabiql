use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget};

use super::command::write_hints;
use super::common::text_field_with_navigation;
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface, KeyContext};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![Row {
        bindings: vec![Binding::action(
            Action::EnterCommandLine,
            &[KeyCombo::plain(Key::Char(':'))],
        )],
        short: "Cmd",
        description: "Open command line",
        dynamic_label: None,
        visible: None,
        surfaces: &[HintSurface::Current, HintSurface::Editing],
    }];
    let (editing, navigation) = text_field_with_navigation(InputTarget::ResultCellEdit);
    rows.extend(editing);
    rows.push(Row {
        bindings: vec![Binding::text(InputTarget::ResultCellEdit)],
        short: "Edit",
        description: "Edit cell value",
        dynamic_label: None,
        visible: None,
        surfaces: &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    });
    rows.extend(navigation);
    rows.push(Row {
        bindings: vec![Binding::action(
            Action::ResultCancelCellEdit,
            &[KeyCombo::plain(Key::Esc)],
        )],
        short: "Normal",
        description: "Exit to Cell Active (draft preserved)",
        dynamic_label: None,
        visible: None,
        surfaces: &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    });
    rows
});

pub fn cell_edit_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn cell_edit_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    let mut result = write_hints(surface, context);
    result.extend(hints(&ROWS, context, surface));
    result
}
