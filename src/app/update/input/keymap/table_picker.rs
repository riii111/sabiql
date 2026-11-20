use std::sync::LazyLock;

use crate::model::shared::settings::KeymapPreset;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget, ListTarget, ModalKind};

use super::common::{list_navigation, text_field};
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface};

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let mut rows = vec![
        Row {
            bindings: vec![Binding::action(
                Action::ConfirmSelection,
                &[KeyCombo::plain(Key::Enter)],
            )],
            short: "Select",
            description: "Select table",
            surfaces: &[HintSurface::Border, HintSurface::Current],
        },
        list_navigation(ListTarget::TablePicker),
    ];
    rows.extend(text_field(InputTarget::Filter));
    rows.push(Row {
        bindings: vec![Binding::text(InputTarget::Filter)],
        short: "Filter",
        description: "Type to filter",
        surfaces: &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::Reference,
        ],
    });
    rows.push(Row {
        bindings: vec![Binding::action(
            Action::CloseModal(ModalKind::TablePicker),
            &[KeyCombo::plain(Key::Esc)],
        )],
        short: "Close",
        description: "Close",
        surfaces: &[HintSurface::Border, HintSurface::Current],
    });
    rows
});

pub fn table_picker_action(combo: KeyCombo) -> Action {
    resolve(&ROWS, combo, KeymapPreset::Default).unwrap_or(Action::None)
}

pub fn table_picker_hints(surface: HintSurface) -> Vec<Hint> {
    hints(&ROWS, KeymapPreset::Default, surface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_field_handles_delete_and_international_input() {
        assert!(matches!(
            table_picker_action(KeyCombo::plain(Key::Delete)),
            Action::TextDelete {
                target: InputTarget::Filter
            }
        ));
        assert!(matches!(
            table_picker_action(KeyCombo::ctrl(Key::Char('z'))),
            Action::None
        ));
        assert!(matches!(
            table_picker_action(KeyCombo::ctrl_alt(Key::Char('z'))),
            Action::TextInput {
                target: InputTarget::Filter,
                ch: 'z'
            }
        ));
    }
}
