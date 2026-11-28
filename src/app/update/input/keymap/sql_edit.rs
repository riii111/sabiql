use std::sync::LazyLock;

use crate::model::shared::settings::KeymapPreset;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};

use super::common::consume_readline;
use super::model::{Binding, Row, hints};
use super::{Hint, HintSurface, KeyContext};

fn completion(context: &KeyContext) -> bool {
    context.is_sql_completion_visible
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
        Row::new(
            vec![
                Binding::with_alt(Action::SqlModalSubmit, Key::Enter),
                Binding::action(Action::SqlModalSubmit, &[KeyCombo::plain(Key::F(5))]),
            ],
            "Run",
            "Execute query",
            border,
        ),
        Row::new(
            vec![Binding::with_ctrl(Action::SqlModalClear, Key::Char('l'))],
            "Clear",
            "Clear editor",
            border,
        ),
        Row::new(
            vec![
                Binding::action(
                    Action::OpenModal(ModalKind::QueryHistoryPicker),
                    &[KeyCombo::ctrl(Key::Char('o'))],
                )
                .ide(&[])
                .when(|context| context.can_open_history),
            ],
            "History",
            "Open Query History",
            border,
        ),
        Row::new(
            vec![
                Binding::action(Action::None, &[KeyCombo::ctrl(Key::Char('o'))])
                    .ide(&[KeyCombo::ctrl(Key::Char('o'))])
                    .consume()
                    .when(|context| {
                        !context.can_open_history || context.preset == KeymapPreset::Ide
                    }),
            ],
            "",
            "",
            &[],
        ),
        Row::new(
            vec![Binding::with_alt(
                Action::ExplainAnalyzeRequest,
                Key::Char('e'),
            )],
            "Analyze",
            "Run EXPLAIN ANALYZE on current query",
            &[HintSurface::Current, HintSurface::Editing],
        ),
        Row::new(
            vec![
                Binding::with_alt(Action::None, Key::Char('e'))
                    .consume()
                    .when(|context| !context.allows(&Action::ExplainAnalyzeRequest)),
            ],
            "",
            "",
            &[],
        ),
        Row::new(
            vec![
                Binding::any_modifiers(Action::CompletionPrev, Key::Up).when(completion),
                Binding::action(Action::CompletionPrev, &[KeyCombo::ctrl(Key::Char('p'))])
                    .when(completion),
                Binding::any_modifiers(Action::CompletionNext, Key::Down).when(completion),
                Binding::action(Action::CompletionNext, &[KeyCombo::ctrl(Key::Char('n'))])
                    .when(completion),
            ],
            "Select",
            "Select completion",
            &[HintSurface::Current, HintSurface::Editing],
        ),
        Row::new(
            vec![
                Binding::any_modifiers(Action::CompletionAccept, Key::Tab).when(completion),
                Binding::any_modifiers(Action::CompletionAccept, Key::Enter).when(completion),
            ],
            "Accept",
            "Accept completion",
            border,
        ),
        Row::new(
            vec![Binding::any_modifiers(Action::CompletionDismiss, Key::Esc).when(completion)],
            "Dismiss",
            "Dismiss completion",
            border,
        ),
        Row::new(
            vec![
                Binding::any_modifiers(Action::CompletionDismiss, Key::Left).when(completion),
                Binding::any_modifiers(Action::CompletionDismiss, Key::Right).when(completion),
            ],
            "",
            "",
            &[],
        ),
        Row::new(
            vec![Binding::no_ctrl_alt(Action::SqlModalEnterNormal, Key::Esc)],
            "Normal",
            "Return to Normal mode",
            border,
        ),
        Row::new(
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
        ),
        Row::new(
            [
                (Key::Home, CursorMove::LineStart),
                (Key::End, CursorMove::LineEnd),
            ]
            .into_iter()
            .map(|(key, direction)| {
                Binding::any_modifiers(Action::TextMoveCursor { target, direction }, key)
            })
            .collect(),
            "Line",
            "Line start/end",
            &[HintSurface::Current, HintSurface::Editing],
        ),
        Row::new(
            vec![Binding::any_modifiers(Action::SqlModalInsertTab, Key::Tab)],
            "Tab",
            "Insert tab",
            &[HintSurface::Current, HintSurface::Editing],
        ),
    ]);
    for (key, action) in [
        (Key::Backspace, Action::TextBackspace { target }),
        (Key::Delete, Action::TextDelete { target }),
        (Key::Enter, Action::SqlModalNewLine),
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

pub fn sql_edit_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    hints(&ROWS, context, surface)
}
