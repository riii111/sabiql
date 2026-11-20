use std::sync::LazyLock;

use crate::model::shared::settings::KeymapPreset;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{
    Action, CursorMove, InputTarget, ListMotion, ListTarget, TextKillDirection,
};

use super::HintSurface;
use super::model::{Binding, Row, resolve};

static READLINE: LazyLock<Vec<(InputTarget, Vec<Row>)>> = LazyLock::new(|| {
    [
        InputTarget::SqlModal,
        InputTarget::SqlModalHighRisk,
        InputTarget::SqlModalAnalyzeHighRisk,
        InputTarget::ResultCellEdit,
        InputTarget::ConnectionSetup,
        InputTarget::CommandLine,
        InputTarget::Filter,
        InputTarget::ErFilter,
        InputTarget::SettingsErBrowser,
        InputTarget::QueryHistoryFilter,
        InputTarget::JsonEdit,
        InputTarget::JsonSearch,
        InputTarget::CellDetailSearch,
        InputTarget::HelpFilter,
    ]
    .into_iter()
    .map(|target| (target, readline_rows(target)))
    .collect()
});

pub(super) fn readline_action_for(combo: &KeyCombo, target: InputTarget) -> Option<Action> {
    let rows = READLINE
        .iter()
        .find(|(candidate, _)| *candidate == target)?;
    resolve(&rows.1, *combo, KeymapPreset::Default)
}

pub(super) fn text_field(target: InputTarget) -> Vec<Row> {
    let mut rows = READLINE
        .iter()
        .find(|(candidate, _)| *candidate == target)
        .map_or_default(|(_, rows)| rows.clone());
    rows.extend([
        hidden(Action::TextBackspace { target }, Key::Backspace),
        hidden(Action::TextDelete { target }, Key::Delete),
        hidden(
            Action::TextMoveCursor {
                target,
                direction: CursorMove::Left,
            },
            Key::Left,
        ),
        hidden(
            Action::TextMoveCursor {
                target,
                direction: CursorMove::Right,
            },
            Key::Right,
        ),
        hidden(
            Action::TextMoveCursor {
                target,
                direction: CursorMove::Home,
            },
            Key::Home,
        ),
        hidden(
            Action::TextMoveCursor {
                target,
                direction: CursorMove::End,
            },
            Key::End,
        ),
    ]);
    rows
}

pub(super) fn list_navigation(target: ListTarget) -> Row {
    Row {
        bindings: vec![
            Binding::action(
                Action::ListSelect {
                    target,
                    motion: ListMotion::Next,
                },
                &[KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))],
            ),
            Binding::action(
                Action::ListSelect {
                    target,
                    motion: ListMotion::Previous,
                },
                &[KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))],
            ),
        ],
        short: "Navigate",
        description: "Navigate",
        surfaces: &[HintSurface::Current],
    }
}

fn hidden(action: Action, key: Key) -> Row {
    hidden_combo(action, KeyCombo::plain(key))
}

fn hidden_combo(action: Action, combo: KeyCombo) -> Row {
    Row {
        bindings: vec![Binding::action(action, &[combo])],
        short: "",
        description: "",
        surfaces: &[],
    }
}

fn readline_rows(target: InputTarget) -> Vec<Row> {
    use CursorMove::{Left, LineEnd, LineStart, ReadlineWordEnd, ReadlineWordStart, Right};
    use TextKillDirection::{
        ReadlinePreviousWhitespace, ReadlineWordEnd as KillWordEnd, ToLineEnd, ToLineStart,
    };
    vec![
        hidden_combo(
            Action::TextMoveCursor {
                target,
                direction: LineStart,
            },
            KeyCombo::ctrl(Key::Char('a')),
        ),
        hidden_combo(
            Action::TextMoveCursor {
                target,
                direction: LineEnd,
            },
            KeyCombo::ctrl(Key::Char('e')),
        ),
        hidden_combo(
            Action::TextMoveCursor {
                target,
                direction: Left,
            },
            KeyCombo::ctrl(Key::Char('b')),
        ),
        hidden_combo(
            Action::TextMoveCursor {
                target,
                direction: Right,
            },
            KeyCombo::ctrl(Key::Char('f')),
        ),
        hidden_combo(
            Action::TextBackspace { target },
            KeyCombo::ctrl(Key::Char('h')),
        ),
        hidden_combo(
            Action::TextDelete { target },
            KeyCombo::ctrl(Key::Char('d')),
        ),
        hidden_combo(
            Action::TextKill {
                target,
                direction: ToLineEnd,
            },
            KeyCombo::ctrl(Key::Char('k')),
        ),
        hidden_combo(
            Action::TextKill {
                target,
                direction: ToLineStart,
            },
            KeyCombo::ctrl(Key::Char('u')),
        ),
        hidden_combo(
            Action::TextKill {
                target,
                direction: ReadlinePreviousWhitespace,
            },
            KeyCombo::ctrl(Key::Char('w')),
        ),
        hidden_combo(Action::TextYank { target }, KeyCombo::ctrl(Key::Char('y'))),
        hidden_combo(
            Action::TextMoveCursor {
                target,
                direction: ReadlineWordStart,
            },
            KeyCombo::alt(Key::Char('b')),
        ),
        hidden_combo(
            Action::TextMoveCursor {
                target,
                direction: ReadlineWordEnd,
            },
            KeyCombo::alt(Key::Char('f')),
        ),
        hidden_combo(
            Action::TextKill {
                target,
                direction: KillWordEnd,
            },
            KeyCombo::alt(Key::Char('d')),
        ),
    ]
}
