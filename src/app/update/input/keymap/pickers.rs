use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ListTarget, ModalKind};

use super::common::{list_navigation, text_field};
use super::model::{Binding, Row, hints, resolve};
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
        &[HintSurface::Border, HintSurface::Current]
    };
    let mut rows = vec![Row {
        bindings: vec![Binding::action(confirm, &[KeyCombo::plain(Key::Enter)])],
        short,
        description,
        visible: None,
        surfaces: button_surfaces,
    }];
    if matches!(picker, Picker::Er) {
        rows.extend([
            Row {
                bindings: vec![Binding::action(
                    Action::ErToggleSelection,
                    &[KeyCombo::plain(Key::Char(' '))],
                )],
                short: "Select",
                description: "Toggle table selection",
                visible: None,
                surfaces: button_surfaces,
            },
            Row {
                bindings: vec![Binding::action(
                    Action::ErSelectAll,
                    &[KeyCombo::alt(Key::Char('a'))],
                )],
                short: "All",
                description: "Select/deselect all tables",
                visible: None,
                surfaces: button_surfaces,
            },
        ]);
    }
    let mut navigation = list_navigation(target, matches!(picker, Picker::CommandPalette));
    if matches!(picker, Picker::Er) {
        navigation.surfaces = &[HintSurface::Current, HintSurface::Advanced];
    }
    rows.push(navigation);
    if let Some(input) = input {
        rows.extend(text_field(input, false));
        rows.push(Row {
            bindings: vec![Binding::text(input)],
            short: "Filter",
            description: "Type to filter",
            visible: None,
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
        visible: None,
        surfaces: button_surfaces,
    });
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

pub fn picker_action(picker: Picker, combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(rows_for(picker), combo, context).unwrap_or(Action::None)
}

pub fn picker_hints(picker: Picker, surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(rows_for(picker), context, surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConnectionId, DatabaseType};
    use crate::model::app_state::AppState;
    use crate::model::shared::input_mode::InputMode;

    #[test]
    fn text_field_handles_delete_and_international_input() {
        let context = KeyContext::from_state(&AppState::new("test".to_string()));
        assert!(matches!(
            picker_action(Picker::Table, KeyCombo::plain(Key::Delete), &context),
            Action::TextDelete {
                target: InputTarget::Filter
            }
        ));
        assert!(matches!(
            picker_action(Picker::Table, KeyCombo::ctrl(Key::Char('z')), &context),
            Action::None
        ));
        assert!(matches!(
            picker_action(Picker::Table, KeyCombo::ctrl_alt(Key::Char('z')), &context),
            Action::TextInput {
                target: InputTarget::Filter,
                ch: 'z'
            }
        ));
    }

    #[test]
    fn er_space_selects_instead_of_inserting_text() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::PostgreSQL,
            "test://database",
        );
        state.modal.set_mode(InputMode::ErTablePicker);
        let context = KeyContext::from_state(&state);

        assert!(matches!(
            picker_action(Picker::Er, KeyCombo::plain(Key::Char(' ')), &context),
            Action::ErToggleSelection
        ));
        assert!(matches!(
            picker_action(Picker::Er, KeyCombo::ctrl_alt(Key::Char(' ')), &context),
            Action::TextInput {
                target: InputTarget::ErFilter,
                ch: ' '
            }
        ));
    }

    #[test]
    fn unavailable_er_operations_are_hidden_and_cannot_execute() {
        let mut state = AppState::new("test".to_string());
        state.session.activate_connection_with_dsn(
            &ConnectionId::new(),
            "database",
            DatabaseType::SQLite,
            "sqlite://test.db",
        );
        state.modal.set_mode(InputMode::ErTablePicker);
        let context = KeyContext::from_state(&state);

        assert!(matches!(
            picker_action(Picker::Er, KeyCombo::plain(Key::Char(' ')), &context),
            Action::None
        ));
        assert!(matches!(
            picker_action(Picker::Er, KeyCombo::plain(Key::Char('a')), &context),
            Action::None
        ));
        assert!(matches!(
            picker_action(Picker::Er, KeyCombo::plain(Key::Esc), &context),
            Action::CloseModal(ModalKind::ErTablePicker)
        ));
        assert!(picker_hints(Picker::Er, HintSurface::Footer, &context).is_empty());
    }
}
