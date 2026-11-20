use std::sync::LazyLock;

use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::settings::KeymapPreset;
use crate::policy::FeaturePolicy;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{
    Action, CursorMove, InputTarget, ListMotion, ListTarget, TextKillDirection,
};

use super::model::{Binding, Row, resolve};
use super::{HintSurface, KeyContext};

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
static READLINE_CONTEXT: LazyLock<KeyContext> = LazyLock::new(|| {
    KeyContext::for_scope(
        InputMode::Normal,
        KeymapPreset::Default,
        &FeaturePolicy::new(&EngineFeatureProfile::postgres_like()),
    )
});

pub(super) fn readline_action_for(combo: &KeyCombo, target: InputTarget) -> Option<Action> {
    let rows = READLINE
        .iter()
        .find(|(candidate, _)| *candidate == target)?;
    resolve(&rows.1, *combo, &READLINE_CONTEXT)
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

pub(super) fn list_navigation(target: ListTarget, vim_keys: bool) -> Row {
    let mut next = vec![KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))];
    let mut previous = vec![KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))];
    if vim_keys {
        next.insert(0, KeyCombo::plain(Key::Char('j')));
        previous.insert(0, KeyCombo::plain(Key::Char('k')));
    }
    Row {
        bindings: vec![
            Binding::action(
                Action::ListSelect {
                    target,
                    motion: ListMotion::Next,
                },
                &next,
            ),
            Binding::action(
                Action::ListSelect {
                    target,
                    motion: ListMotion::Previous,
                },
                &previous,
            ),
        ],
        short: "Navigate",
        description: "Navigate",
        visible: None,
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
        visible: None,
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
