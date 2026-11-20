use std::sync::LazyLock;

use crate::model::shared::settings::KeymapPreset;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ListTarget, ModalKind};

use super::common::{list_navigation, text_field};
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface};

pub(super) static TABLE_ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| rows(Picker::Table));
pub(super) static HISTORY_ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| rows(Picker::QueryHistory));
pub(super) static PALETTE_ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| rows(Picker::CommandPalette));

#[derive(Clone, Copy)]
pub enum Picker {
    Table,
    QueryHistory,
    CommandPalette,
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
                HintSurface::Reference,
            ][..],
        ),
        Picker::QueryHistory => (
            ListTarget::QueryHistory,
            Some(InputTarget::QueryHistoryFilter),
            Action::QueryHistoryConfirmSelection,
            ModalKind::QueryHistoryPicker,
            "Select",
            "Select query",
            &[HintSurface::Current, HintSurface::Reference][..],
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
    };
    let mut rows = vec![
        Row {
            bindings: vec![Binding::action(confirm, &[KeyCombo::plain(Key::Enter)])],
            short,
            description,
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
        list_navigation(target, matches!(picker, Picker::CommandPalette)),
    ];
    if let Some(input) = input {
        rows.extend(text_field(input));
        rows.push(Row {
            bindings: vec![Binding::text(input)],
            short: "Filter",
            description: "Type to filter",
            surfaces: filter_surfaces,
        });
    }
    rows.push(Row {
        bindings: vec![Binding::action(
            Action::CloseModal(close),
            &[KeyCombo::plain(Key::Esc)],
        )],
        short: "Close",
        description: "Close",
        surfaces: &[HintSurface::Border, HintSurface::Current],
    });
    rows
}

fn rows_for(picker: Picker) -> &'static [Row] {
    match picker {
        Picker::Table => &TABLE_ROWS,
        Picker::QueryHistory => &HISTORY_ROWS,
        Picker::CommandPalette => &PALETTE_ROWS,
    }
}

pub fn picker_action(picker: Picker, combo: KeyCombo) -> Action {
    resolve(rows_for(picker), combo, KeymapPreset::Default).unwrap_or(Action::None)
}

pub fn picker_hints(picker: Picker, surface: HintSurface) -> Vec<Hint> {
    hints(rows_for(picker), KeymapPreset::Default, surface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_field_handles_delete_and_international_input() {
        assert!(matches!(
            picker_action(Picker::Table, KeyCombo::plain(Key::Delete)),
            Action::TextDelete {
                target: InputTarget::Filter
            }
        ));
        assert!(matches!(
            picker_action(Picker::Table, KeyCombo::ctrl(Key::Char('z'))),
            Action::None
        ));
        assert!(matches!(
            picker_action(Picker::Table, KeyCombo::ctrl_alt(Key::Char('z'))),
            Action::TextInput {
                target: InputTarget::Filter,
                ch: 'z'
            }
        ));
    }
}
