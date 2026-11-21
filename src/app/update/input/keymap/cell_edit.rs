use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget};

use super::HintSurface;
use super::command::cell_edit_write_row;
use super::common::text_field_with_navigation;
use super::model::{Binding, Row};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![
        cell_edit_write_row(),
        Row::new(
            vec![Binding::action(
                Action::EnterCommandLine,
                &[KeyCombo::plain(Key::Char(':'))],
            )],
            "Cmd",
            "Open command line",
            &[HintSurface::Current, HintSurface::Editing],
        ),
    ];
    let (editing, navigation) = text_field_with_navigation(InputTarget::ResultCellEdit);
    rows.extend(editing);
    rows.push(Row::new(
        vec![Binding::text(InputTarget::ResultCellEdit)],
        "Edit",
        "Edit cell value",
        &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    ));
    rows.extend(navigation);
    rows.push(Row::new(
        vec![Binding::action(
            Action::ResultCancelCellEdit,
            &[KeyCombo::plain(Key::Esc)],
        )],
        "Normal",
        "Exit to Cell Active (draft preserved)",
        &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Editing,
        ],
    ));
    rows
});
