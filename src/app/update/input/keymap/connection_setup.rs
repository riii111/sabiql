use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, InputTarget};

use super::common::conditional_text_field;
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface, KeyContext};

static CONNECT: LazyLock<Vec<Row>> = LazyLock::new(|| rows("Connect"));
static SAVE: LazyLock<Vec<Row>> = LazyLock::new(|| rows("Save"));

fn form(context: &KeyContext) -> bool {
    !context.is_setup_dropdown_open()
}
fn text(context: &KeyContext) -> bool {
    form(context) && context.is_setup_text_focused()
}
fn closed_dropdown(context: &KeyContext) -> bool {
    form(context) && context.is_setup_dropdown_field()
}
fn open_dropdown(context: &KeyContext) -> bool {
    context.is_setup_dropdown_open()
}

fn rows(submit: &'static str) -> Vec<Row> {
    let border = &[
        HintSurface::Border,
        HintSurface::Current,
        HintSurface::Connections,
    ];
    let mut rows = vec![
        Row::new(
            vec![
                Binding::any_modifiers(Action::ConnectionSetupNextField, Key::Tab).when(form),
                Binding::any_modifiers(Action::ConnectionSetupPrevField, Key::BackTab).when(form),
            ],
            "Field",
            "Next/Previous field",
            border,
        ),
        Row::new(
            vec![
                Binding::any_modifiers(Action::ConnectionSetupToggleDropdown, Key::Enter)
                    .when(closed_dropdown),
            ],
            "Toggle",
            "Toggle dropdown",
            border,
        ),
        Row::new(
            vec![
                Binding::any_modifiers(Action::ConnectionSetupDropdownConfirm, Key::Enter)
                    .when(open_dropdown),
            ],
            "Select",
            "Confirm dropdown selection",
            border,
        ),
        Row::new(
            vec![
                Binding::action(
                    Action::ConnectionSetupSave,
                    &[KeyCombo::ctrl(Key::Char('s'))],
                )
                .ide(&[KeyCombo::plain(Key::Enter), KeyCombo::ctrl(Key::Char('s'))]),
            ],
            submit,
            "Save and connect",
            border,
        ),
        Row::new(
            vec![
                Binding::any_modifiers(Action::ConnectionSetupDropdownNext, Key::Down)
                    .when(open_dropdown),
                Binding::action(
                    Action::ConnectionSetupDropdownNext,
                    &[KeyCombo::ctrl(Key::Char('n'))],
                )
                .when(open_dropdown),
                Binding::any_modifiers(Action::ConnectionSetupDropdownPrev, Key::Up)
                    .when(open_dropdown),
                Binding::action(
                    Action::ConnectionSetupDropdownPrev,
                    &[KeyCombo::ctrl(Key::Char('p'))],
                )
                .when(open_dropdown),
            ],
            "Select",
            "Dropdown navigation",
            &[HintSurface::Current, HintSurface::Connections],
        ),
        Row::new(
            vec![
                Binding::any_modifiers(Action::ConnectionSetupDropdownCancel, Key::Esc)
                    .when(open_dropdown),
            ],
            "Cancel",
            "Cancel dropdown",
            border,
        ),
        Row::new(
            vec![Binding::any_modifiers(Action::ConnectionSetupCancel, Key::Esc).when(form)],
            "Cancel",
            "Cancel",
            border,
        ),
    ];
    rows.extend(conditional_text_field(
        InputTarget::ConnectionSetup,
        form,
        text,
    ));
    rows.push(Row::new(
        vec![Binding::text(InputTarget::ConnectionSetup).when(form)],
        "Edit",
        "Edit connection setting",
        &[],
    ));
    rows
}

pub(super) fn active_rows(context: &KeyContext) -> &'static [Row] {
    if context.is_setup_editing() {
        &SAVE
    } else {
        &CONNECT
    }
}

pub fn connection_setup_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(active_rows(context), combo, context).unwrap_or(Action::None)
}

pub fn connection_setup_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(active_rows(context), context, surface)
}
