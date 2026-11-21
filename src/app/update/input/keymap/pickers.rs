use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ListTarget, ModalKind};

use super::common::{list_navigation, text_field};
use super::model::{Binding, Row, hints};
use super::{Hint, HintSurface, KeyContext};

pub(super) static TABLE_ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| rows(Picker::Table));
pub(super) static HISTORY_ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| rows(Picker::QueryHistory));
pub(super) static PALETTE_ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| rows(Picker::CommandPalette));
pub(super) static ER_ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| rows(Picker::Er));

#[derive(Clone, Copy)]
pub enum Picker {
    Table,
    QueryHistory,
    CommandPalette,
    Er,
}

fn rows(picker: Picker) -> Vec<Row> {
    let (target, input, confirm, close, short, description, filter_surfaces) = match picker {
        Picker::Table => (
            ListTarget::TablePicker,
            Some(InputTarget::Filter),
            Action::ConfirmSelection,
            ModalKind::TablePicker,
            "Select",
            "Select table",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::SearchFilter,
            ][..],
        ),
        Picker::QueryHistory => (
            ListTarget::QueryHistory,
            Some(InputTarget::QueryHistoryFilter),
            Action::QueryHistoryConfirmSelection,
            ModalKind::QueryHistoryPicker,
            "Select",
            "Select query",
            &[HintSurface::Current, HintSurface::SearchFilter][..],
        ),
        Picker::CommandPalette => (
            ListTarget::CommandPalette,
            None,
            Action::ConfirmSelection,
            ModalKind::CommandPalette,
            "Execute",
            "Execute command",
            &[][..],
        ),
        Picker::Er => (
            ListTarget::ErTablePicker,
            Some(InputTarget::ErFilter),
            Action::ErConfirmSelection,
            ModalKind::ErTablePicker,
            "Generate",
            "Generate ER diagram",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::SearchFilter,
                HintSurface::Advanced,
            ][..],
        ),
    };
    let button_surfaces: &'static [HintSurface] = if matches!(picker, Picker::Er) {
        &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::Advanced,
        ]
    } else {
        &[
            HintSurface::Border,
            HintSurface::Current,
            HintSurface::ModalBasics,
        ]
    };
    let mut rows = vec![Row::new(
        vec![Binding::action(confirm, &[KeyCombo::plain(Key::Enter)])],
        short,
        description,
        button_surfaces,
    )];
    if matches!(picker, Picker::Er) {
        rows.extend([
            Row::new(
                vec![Binding::action(
                    Action::ErToggleSelection,
                    &[KeyCombo::plain(Key::Char(' '))],
                )],
                "Select",
                "Toggle table selection",
                button_surfaces,
            ),
            Row::new(
                vec![Binding::action(
                    Action::ErSelectAll,
                    &[KeyCombo::alt(Key::Char('a'))],
                )],
                "All",
                "Select/deselect all tables",
                button_surfaces,
            ),
        ]);
    }
    let mut navigation = list_navigation(target, matches!(picker, Picker::CommandPalette));
    if matches!(picker, Picker::Er) {
        navigation.surfaces = &[HintSurface::Current, HintSurface::Advanced];
    }
    rows.push(navigation);
    if let Some(input) = input {
        rows.extend(text_field(input, false));
        rows.push(Row::new(
            vec![Binding::text(input)],
            "Filter",
            "Type to filter",
            filter_surfaces,
        ));
    }
    rows.push(Row::new(
        vec![Binding::action(
            Action::CloseModal(close),
            &[KeyCombo::plain(Key::Esc)],
        )],
        "Close",
        "Close",
        button_surfaces,
    ));
    rows
}

fn rows_for(picker: Picker) -> &'static [Row] {
    match picker {
        Picker::Table => &TABLE_ROWS,
        Picker::QueryHistory => &HISTORY_ROWS,
        Picker::CommandPalette => &PALETTE_ROWS,
        Picker::Er => &ER_ROWS,
    }
}

pub fn picker_hints(picker: Picker, surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(rows_for(picker), context, surface)
}
