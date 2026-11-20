use std::sync::LazyLock;

use crate::model::shared::settings::KeymapPreset;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ListTarget, ModalKind};

use super::common::{list_navigation, text_field};
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface};

pub(super) static TABLE_ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| rows(TextPicker::Table));
pub(super) static HISTORY_ROWS: LazyLock<Vec<Row>> =
    LazyLock::new(|| rows(TextPicker::QueryHistory));

#[derive(Clone, Copy)]
pub enum TextPicker {
    Table,
    QueryHistory,
}

fn rows(picker: TextPicker) -> Vec<Row> {
    let (target, input, confirm, close, description, filter_surfaces) = match picker {
        TextPicker::Table => (
            ListTarget::TablePicker,
            InputTarget::Filter,
            Action::ConfirmSelection,
            ModalKind::TablePicker,
            "Select table",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Reference,
            ][..],
        ),
        TextPicker::QueryHistory => (
            ListTarget::QueryHistory,
            InputTarget::QueryHistoryFilter,
            Action::QueryHistoryConfirmSelection,
            ModalKind::QueryHistoryPicker,
            "Select query",
            &[HintSurface::Current, HintSurface::Reference][..],
        ),
    };
    let mut rows = vec![
        Row {
            bindings: vec![Binding::action(confirm, &[KeyCombo::plain(Key::Enter)])],
            short: "Select",
            description,
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
        list_navigation(target),
    ];
    rows.extend(text_field(input));
    rows.push(Row {
        bindings: vec![Binding::text(input)],
        short: "Filter",
        description: "Type to filter",
        surfaces: filter_surfaces,
    });
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

fn rows_for(picker: TextPicker) -> &'static [Row] {
    match picker {
        TextPicker::Table => &TABLE_ROWS,
        TextPicker::QueryHistory => &HISTORY_ROWS,
    }
}

pub fn picker_action(picker: TextPicker, combo: KeyCombo) -> Action {
    resolve(rows_for(picker), combo, KeymapPreset::Default).unwrap_or(Action::None)
}

pub fn picker_hints(picker: TextPicker, surface: HintSurface) -> Vec<Hint> {
    hints(rows_for(picker), KeymapPreset::Default, surface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_field_handles_delete_and_international_input() {
        assert!(matches!(
            picker_action(TextPicker::Table, KeyCombo::plain(Key::Delete)),
            Action::TextDelete {
                target: InputTarget::Filter
            }
        ));
        assert!(matches!(
            picker_action(TextPicker::Table, KeyCombo::ctrl(Key::Char('z'))),
            Action::None
        ));
        assert!(matches!(
            picker_action(TextPicker::Table, KeyCombo::ctrl_alt(Key::Char('z'))),
            Action::TextInput {
                target: InputTarget::Filter,
                ch: 'z'
            }
        ));
    }
}
