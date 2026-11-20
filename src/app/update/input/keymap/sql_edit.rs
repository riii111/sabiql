use std::sync::LazyLock;

use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};

use super::common::consume_readline;
use super::model::{Binding, Row, hints, resolve};
use super::{Hint, HintSurface, KeyContext};

fn completion(context: &KeyContext) -> bool {
    context.is_sql_completion_visible()
}

pub(super) static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let target = InputTarget::SqlModal;
    let border = &[
        HintSurface::Border,
        HintSurface::Current,
        HintSurface::Editing,
    ];
    let mut rows = consume_readline(target);
    rows.extend([
        Row {
            bindings: vec![
                Binding::with_alt(Action::SqlModalSubmit, Key::Enter),
                Binding::action(Action::SqlModalSubmit, &[KeyCombo::plain(Key::F(5))]),
            ],
            short: "Run",
            description: "Execute query",
            visible: None,
            surfaces: border,
        },
        Row {
            bindings: vec![Binding::with_ctrl(Action::SqlModalClear, Key::Char('l'))],
            short: "Clear",
            description: "Clear editor",
            visible: None,
            surfaces: border,
        },
        Row {
            bindings: vec![
                Binding::action(
                    Action::OpenModal(ModalKind::QueryHistoryPicker),
                    &[KeyCombo::ctrl(Key::Char('o'))],
                )
                .ide(&[]),
            ],
            short: "History",
            description: "Open Query History",
            visible: None,
            surfaces: border,
        },
        Row {
            bindings: vec![
                Binding::action(Action::None, &[])
                    .ide(&[KeyCombo::ctrl(Key::Char('o'))])
                    .consume(),
            ],
            short: "",
            description: "",
            visible: None,
            surfaces: &[],
        },
        Row {
            bindings: vec![Binding::with_alt(
                Action::ExplainAnalyzeRequest,
                Key::Char('e'),
            )],
            short: "Analyze",
            description: "Run EXPLAIN ANALYZE on current query",
            visible: None,
            surfaces: &[HintSurface::Current, HintSurface::Editing],
        },
        Row {
            bindings: vec![Binding::with_alt(Action::None, Key::Char('e')).consume()],
            short: "",
            description: "",
            visible: None,
            surfaces: &[],
        },
        Row {
            bindings: vec![
                Binding::any_modifiers(Action::CompletionPrev, Key::Up).when(completion),
                Binding::action(Action::CompletionPrev, &[KeyCombo::ctrl(Key::Char('p'))])
                    .when(completion),
                Binding::any_modifiers(Action::CompletionNext, Key::Down).when(completion),
                Binding::action(Action::CompletionNext, &[KeyCombo::ctrl(Key::Char('n'))])
                    .when(completion),
            ],
            short: "Select",
            description: "Select completion",
            visible: None,
            surfaces: &[HintSurface::Current],
        },
        Row {
            bindings: vec![
                Binding::any_modifiers(Action::CompletionAccept, Key::Tab).when(completion),
                Binding::any_modifiers(Action::CompletionAccept, Key::Enter).when(completion),
            ],
            short: "Accept",
            description: "Accept completion",
            visible: None,
            surfaces: border,
        },
        Row {
            bindings: vec![
                Binding::any_modifiers(Action::CompletionDismiss, Key::Esc).when(completion),
            ],
            short: "Dismiss",
            description: "Dismiss completion",
            visible: None,
            surfaces: border,
        },
        Row {
            bindings: vec![
                Binding::any_modifiers(Action::CompletionDismiss, Key::Left).when(completion),
                Binding::any_modifiers(Action::CompletionDismiss, Key::Right).when(completion),
            ],
            short: "",
            description: "",
            visible: None,
            surfaces: &[],
        },
        Row {
            bindings: vec![Binding::no_ctrl_alt(Action::SqlModalEnterNormal, Key::Esc)],
            short: "Normal",
            description: "Return to Normal mode",
            visible: None,
            surfaces: border,
        },
        Row {
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
            visible: None,
            surfaces: &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Editing,
            ],
        },
        Row {
            bindings: [(Key::Home, CursorMove::Home), (Key::End, CursorMove::End)]
                .into_iter()
                .map(|(key, direction)| {
                    Binding::any_modifiers(Action::TextMoveCursor { target, direction }, key)
                })
                .collect(),
            short: "Line",
            description: "Line start/end",
            visible: None,
            surfaces: &[HintSurface::Current, HintSurface::Editing],
        },
        Row {
            bindings: vec![Binding::any_modifiers(Action::SqlModalInsertTab, Key::Tab)],
            short: "Tab",
            description: "Insert tab",
            visible: None,
            surfaces: &[HintSurface::Current, HintSurface::Editing],
        },
    ]);
    for (key, action) in [
        (Key::Backspace, Action::TextBackspace { target }),
        (Key::Delete, Action::TextDelete { target }),
        (Key::Enter, Action::SqlModalNewLine),
    ] {
        rows.push(Row {
            bindings: vec![Binding::any_modifiers(action, key)],
            short: "",
            description: "",
            visible: None,
            surfaces: &[],
        });
    }
    rows.push(Row {
        bindings: vec![Binding::text_any_modifiers(target)],
        short: "",
        description: "",
        visible: None,
        surfaces: &[],
    });
    rows
});

pub fn sql_edit_action(combo: KeyCombo, context: &KeyContext) -> Action {
    resolve(&ROWS, combo, context).unwrap_or(Action::None)
}

pub fn sql_edit_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
