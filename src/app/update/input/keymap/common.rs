use std::sync::LazyLock;

use crate::model::shared::engine_feature_profile::EngineFeatureProfile;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::Prefix;
use crate::model::shared::settings::KeymapPreset;
use crate::policy::FeaturePolicy;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{
    Action, CursorMove, DetailTarget, InputTarget, ListMotion, ListTarget, SearchStep,
    TextKillDirection,
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

pub(super) fn text_field(target: InputTarget, shift_keys: bool) -> Vec<Row> {
    text_field_with(
        target,
        if shift_keys {
            KeyModifiers::Shift
        } else {
            KeyModifiers::None
        },
    )
}

pub(super) fn text_field_any_modifiers(target: InputTarget) -> Vec<Row> {
    text_field_with(target, KeyModifiers::Any)
}

pub(super) fn confirmation_text_field(target: InputTarget) -> Vec<Row> {
    let mut rows = text_field(target, true);
    let first = rows.len() - 4;
    for (row, key) in rows[first..]
        .iter_mut()
        .zip([Key::Left, Key::Right, Key::Home, Key::End])
    {
        for binding in &mut row.bindings {
            *binding = binding.clone().with_any_modifiers(key);
        }
    }
    rows
}

pub(super) fn consume_readline(target: InputTarget) -> Vec<Row> {
    let mut rows = READLINE
        .iter()
        .find(|(candidate, _)| *candidate == target)
        .map_or_default(|(_, rows)| rows.clone());
    for row in &mut rows {
        for binding in &mut row.bindings {
            *binding = binding.clone().consume();
        }
    }
    rows
}

pub(super) fn detail_search(
    target: DetailTarget,
    input: InputTarget,
    any_modifiers: bool,
) -> Vec<Row> {
    let mut rows = vec![
        Row {
            bindings: vec![Binding::action(
                Action::DetailSearch {
                    target,
                    step: SearchStep::Submit,
                },
                &[KeyCombo::plain(Key::Enter)],
            )],
            short: "Confirm",
            description: "Confirm search",
            dynamic_label: None,
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
            ],
        },
        Row {
            bindings: vec![Binding::action(
                Action::DetailSearch {
                    target,
                    step: SearchStep::Exit,
                },
                &[KeyCombo::plain(Key::Esc)],
            )],
            short: "Cancel",
            description: "Cancel search",
            dynamic_label: None,
            visible: None,
            surfaces: &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
            ],
        },
    ];
    rows.extend(if any_modifiers {
        text_field_any_modifiers(input)
    } else {
        text_field(input, true)
    });
    rows.push(Row {
        bindings: vec![Binding::text(input)],
        short: "Search",
        description: "Type to search",
        dynamic_label: None,
        visible: None,
        surfaces: &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::SearchFilter,
        ],
    });
    rows
}

pub(super) fn document_navigation(target: InputTarget, shifted_prefix: bool) -> Vec<Row> {
    let movement = |direction, combos: &[KeyCombo]| {
        Binding::action(Action::TextMoveCursor { target, direction }, combos)
    };
    let arrow = |direction, combos: &[KeyCombo], key| {
        let binding = movement(direction, combos);
        if target == InputTarget::SqlModal {
            binding.add_any_modifiers(key)
        } else {
            let mut all = combos.to_vec();
            all.insert(1, KeyCombo::plain(key));
            movement(direction, &all)
        }
    };
    let mut jumps = vec![
        movement(CursorMove::LineStart, &[KeyCombo::plain(Key::Char('0'))]),
        movement(CursorMove::LineEnd, &[KeyCombo::plain(Key::Char('$'))]),
        movement(CursorMove::WordForward, &[KeyCombo::plain(Key::Char('w'))]),
        movement(CursorMove::WordBackward, &[KeyCombo::plain(Key::Char('b'))]),
    ];
    if target == InputTarget::SqlModal {
        jumps.extend([
            movement(CursorMove::Home, &[KeyCombo::plain(Key::Home)]),
            movement(CursorMove::End, &[KeyCombo::plain(Key::End)]),
        ]);
    } else {
        jumps[0] = jumps[0].clone().with_no_ctrl_alt(Key::Home);
        jumps[1] = jumps[1].clone().with_no_ctrl_alt(Key::End);
    }
    vec![
        Row {
            bindings: vec![
                arrow(
                    CursorMove::Left,
                    &[KeyCombo::plain(Key::Char('h'))],
                    Key::Left,
                ),
                arrow(
                    CursorMove::Right,
                    &[KeyCombo::plain(Key::Char('l'))],
                    Key::Right,
                ),
            ],
            short: "Left/Right",
            description: "Move cursor left / right",
            dynamic_label: None,
            visible: None,
            surfaces: &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
        Row {
            bindings: vec![
                arrow(
                    CursorMove::Down,
                    &[
                        KeyCombo::plain(Key::Char('j')),
                        KeyCombo::ctrl(Key::Char('n')),
                    ],
                    Key::Down,
                ),
                arrow(
                    CursorMove::Up,
                    &[
                        KeyCombo::plain(Key::Char('k')),
                        KeyCombo::ctrl(Key::Char('p')),
                    ],
                    Key::Up,
                ),
            ],
            short: "Down/Up",
            description: "Move cursor down / up",
            dynamic_label: None,
            visible: None,
            surfaces: &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        },
        Row {
            bindings: jumps,
            short: "Jump",
            description: "Move by word or line boundary",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Current, HintSurface::Advanced],
        },
        Row {
            bindings: vec![
                Binding::sequence(
                    Action::TextMoveCursor {
                        target,
                        direction: CursorMove::FirstLine,
                    },
                    Prefix::G,
                    KeyCombo::plain(Key::Char('g')),
                    shifted_prefix,
                ),
                movement(CursorMove::LastLine, &[KeyCombo::plain(Key::Char('G'))]),
                movement(CursorMove::ViewportTop, &[KeyCombo::plain(Key::Char('H'))]),
                movement(
                    CursorMove::ViewportMiddle,
                    &[KeyCombo::plain(Key::Char('M'))],
                ),
                movement(
                    CursorMove::ViewportBottom,
                    &[KeyCombo::plain(Key::Char('L'))],
                ),
            ],
            short: "View",
            description: "Jump by buffer or viewport",
            dynamic_label: None,
            visible: None,
            surfaces: &[HintSurface::Current, HintSurface::Advanced],
        },
    ]
}

pub(super) fn conditional_text_field(
    target: InputTarget,
    editing: fn(&KeyContext) -> bool,
    readline: fn(&KeyContext) -> bool,
) -> Vec<Row> {
    let readline_count = READLINE
        .iter()
        .find(|(candidate, _)| *candidate == target)
        .map_or(0, |(_, rows)| rows.len());
    let mut rows = text_field_any_modifiers(target);
    for (index, row) in rows.iter_mut().enumerate() {
        let condition = if index < readline_count {
            readline
        } else {
            editing
        };
        for binding in &mut row.bindings {
            *binding = binding.clone().when(condition);
        }
    }
    rows
}

pub(super) fn text_field_with_navigation(target: InputTarget) -> (Vec<Row>, Vec<Row>) {
    let mut rows = text_field_any_modifiers(target);
    let first = rows.len() - 4;
    let mut left = rows.remove(first);
    let right = rows.remove(first);
    let mut home = rows.remove(first);
    let end = rows.remove(first);
    left.bindings.extend(right.bindings);
    left.short = "Move";
    left.description = "Move cursor";
    left.surfaces = &[
        HintSurface::Footer,
        HintSurface::Current,
        HintSurface::Editing,
    ];
    home.bindings.extend(end.bindings);
    home.short = "Jump";
    home.description = "Jump to start/end";
    home.surfaces = &[HintSurface::Current, HintSurface::Editing];
    (rows, vec![left, home])
}

#[derive(Clone, Copy)]
enum KeyModifiers {
    None,
    Shift,
    Any,
}

fn text_field_with(target: InputTarget, modifiers: KeyModifiers) -> Vec<Row> {
    let mut rows = READLINE
        .iter()
        .find(|(candidate, _)| *candidate == target)
        .map_or_default(|(_, rows)| rows.clone());
    rows.extend([
        hidden(Action::TextBackspace { target }, Key::Backspace, modifiers),
        hidden(Action::TextDelete { target }, Key::Delete, modifiers),
        hidden(
            Action::TextMoveCursor {
                target,
                direction: CursorMove::Left,
            },
            Key::Left,
            modifiers,
        ),
        hidden(
            Action::TextMoveCursor {
                target,
                direction: CursorMove::Right,
            },
            Key::Right,
            modifiers,
        ),
        hidden(
            Action::TextMoveCursor {
                target,
                direction: CursorMove::Home,
            },
            Key::Home,
            modifiers,
        ),
        hidden(
            Action::TextMoveCursor {
                target,
                direction: CursorMove::End,
            },
            Key::End,
            modifiers,
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
        dynamic_label: None,
        visible: None,
        surfaces: &[HintSurface::Current],
    }
}

fn hidden(action: Action, key: Key, modifiers: KeyModifiers) -> Row {
    let combos = [KeyCombo::plain(key), KeyCombo::shift(key)];
    let binding = match modifiers {
        KeyModifiers::None => Binding::action(action, &combos[..1]),
        KeyModifiers::Shift => Binding::action(action, &combos),
        KeyModifiers::Any => Binding::any_modifiers(action, key),
    };
    Row {
        bindings: vec![binding],
        short: "",
        description: "",
        dynamic_label: None,
        visible: None,
        surfaces: &[],
    }
}

fn hidden_combo(action: Action, combo: KeyCombo) -> Row {
    hidden_binding(action, &[combo])
}

fn hidden_binding(action: Action, combos: &[KeyCombo]) -> Row {
    Row {
        bindings: vec![Binding::action(action, combos)],
        short: "",
        description: "",
        dynamic_label: None,
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
