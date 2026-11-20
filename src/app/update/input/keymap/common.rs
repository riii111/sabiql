use crate::model::shared::key_sequence::Prefix;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{
    Action, CursorMove, DetailTarget, InputTarget, ListMotion, ListTarget, SearchStep,
    TextKillDirection,
};

use super::model::{Binding, Row};
use super::{HintSurface, KeyContext};

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
    let mut rows = readline_rows(target);
    rows.extend(deletion_rows(target, KeyModifiers::Shift));
    rows.extend(cursor_rows(target, KeyModifiers::Any));
    rows
}

pub(super) fn consume_readline(target: InputTarget) -> Vec<Row> {
    let mut rows = readline_rows(target);
    for row in &mut rows {
        row.references = &[];
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
        Row::new(
            vec![Binding::action(
                Action::DetailSearch {
                    target,
                    step: SearchStep::Submit,
                },
                &[KeyCombo::plain(Key::Enter)],
            )],
            "Confirm",
            "Confirm search",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
            ],
        ),
        Row::new(
            vec![Binding::action(
                Action::DetailSearch {
                    target,
                    step: SearchStep::Exit,
                },
                &[KeyCombo::plain(Key::Esc)],
            )],
            "Cancel",
            "Cancel search",
            &[
                HintSurface::Border,
                HintSurface::Current,
                HintSurface::SearchFilter,
            ],
        ),
    ];
    rows.extend(if any_modifiers {
        text_field_any_modifiers(input)
    } else {
        text_field(input, true)
    });
    rows.push(Row::new(
        vec![Binding::text(input)],
        "Search",
        "Type to search",
        &[
            HintSurface::Footer,
            HintSurface::Current,
            HintSurface::SearchFilter,
        ],
    ));
    rows
}

#[derive(Clone, Copy)]
pub(super) enum NavigationKey {
    Down,
    Up,
    Left,
    Right,
    First,
    Last,
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy)]
pub(super) enum NavigationLayout {
    Browse,
    Document { sql: bool, shifted_prefix: bool },
}

pub(super) fn navigation_binding(
    layout: NavigationLayout,
    movement: NavigationKey,
    action: Action,
) -> Binding {
    use NavigationKey::{Bottom, Down, First, Last, Left, Middle, Right, Top, Up};
    let directional = match movement {
        Down => Some(('j', Key::Down, Some('n'))),
        Up => Some(('k', Key::Up, Some('p'))),
        Left => Some(('h', Key::Left, None)),
        Right => Some(('l', Key::Right, None)),
        _ => None,
    };
    if let Some((ch, arrow, control)) = directional {
        let mut combos = vec![KeyCombo::plain(Key::Char(ch))];
        let any_arrow = matches!(layout, NavigationLayout::Document { sql: true, .. });
        if !any_arrow {
            combos.push(KeyCombo::plain(arrow));
        }
        if let (NavigationLayout::Document { .. }, Some(control)) = (layout, control) {
            combos.push(KeyCombo::ctrl(Key::Char(control)));
        }
        let binding = Binding::action(action, &combos);
        return if any_arrow {
            binding.add_any_modifiers(arrow)
        } else {
            binding
        };
    }
    match (layout, movement) {
        (NavigationLayout::Document { shifted_prefix, .. }, First) => Binding::sequence(
            action,
            Prefix::G,
            KeyCombo::plain(Key::Char('g')),
            shifted_prefix,
        ),
        (NavigationLayout::Browse, First) => Binding::action(
            action,
            &[KeyCombo::plain(Key::Char('g')), KeyCombo::plain(Key::Home)],
        ),
        (NavigationLayout::Browse, Last) => Binding::action(
            action,
            &[KeyCombo::plain(Key::Char('G')), KeyCombo::plain(Key::End)],
        ),
        (NavigationLayout::Document { .. }, Last) => {
            Binding::action(action, &[KeyCombo::plain(Key::Char('G'))])
        }
        (_, Top) => Binding::action(action, &[KeyCombo::plain(Key::Char('H'))]),
        (_, Middle) => Binding::action(action, &[KeyCombo::plain(Key::Char('M'))]),
        (_, Bottom) => Binding::action(action, &[KeyCombo::plain(Key::Char('L'))]),
        (_, Down | Up | Left | Right) => unreachable!(),
    }
}

pub(super) fn document_navigation(target: InputTarget, shifted_prefix: bool) -> Vec<Row> {
    use NavigationKey::{Bottom, Down, First, Last, Left, Middle, Right, Top, Up};
    let movement = |direction, combos: &[KeyCombo]| {
        Binding::action(Action::TextMoveCursor { target, direction }, combos)
    };
    let layout = NavigationLayout::Document {
        sql: target == InputTarget::SqlModal,
        shifted_prefix,
    };
    let navigate = |key, direction| {
        navigation_binding(layout, key, Action::TextMoveCursor { target, direction })
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
        Row::new(
            vec![
                navigate(Left, CursorMove::Left),
                navigate(Right, CursorMove::Right),
            ],
            "Left/Right",
            "Move cursor left / right",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
        Row::new(
            vec![
                navigate(Down, CursorMove::Down),
                navigate(Up, CursorMove::Up),
            ],
            "Down/Up",
            "Move cursor down / up",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Advanced,
            ],
        ),
        Row::new(
            jumps,
            "Jump",
            "Move by word or line boundary",
            &[HintSurface::Current, HintSurface::Advanced],
        ),
        Row::new(
            vec![
                navigate(First, CursorMove::FirstLine),
                navigate(Last, CursorMove::LastLine),
                navigate(Top, CursorMove::ViewportTop),
                navigate(Middle, CursorMove::ViewportMiddle),
                navigate(Bottom, CursorMove::ViewportBottom),
            ],
            "View",
            "Jump by buffer or viewport",
            &[HintSurface::Current, HintSurface::Advanced],
        ),
    ]
}

pub(super) fn conditional_text_field(
    target: InputTarget,
    editing: fn(&KeyContext) -> bool,
    readline: fn(&KeyContext) -> bool,
) -> Vec<Row> {
    let mut rows = readline_rows(target);
    let mut other = deletion_rows(target, KeyModifiers::Any);
    other.extend(cursor_rows(target, KeyModifiers::Any));
    for (row, condition) in rows
        .iter_mut()
        .map(|row| (row, readline))
        .chain(other.iter_mut().map(|row| (row, editing)))
    {
        for binding in &mut row.bindings {
            *binding = binding.clone().when(condition);
        }
    }
    rows.extend(other);
    rows
}

pub(super) fn text_field_with_navigation(target: InputTarget) -> (Vec<Row>, Vec<Row>) {
    let mut rows = readline_rows(target);
    rows.extend(deletion_rows(target, KeyModifiers::Any));
    let group = |keys: &[(Key, CursorMove)], short, description, surfaces| {
        Row::new(
            keys.iter()
                .map(|&(key, direction)| {
                    Binding::any_modifiers(Action::TextMoveCursor { target, direction }, key)
                })
                .collect(),
            short,
            description,
            surfaces,
        )
    };
    let navigation = vec![
        group(
            &[
                (Key::Left, CursorMove::Left),
                (Key::Right, CursorMove::Right),
            ],
            "Move",
            "Move cursor",
            &[
                HintSurface::Footer,
                HintSurface::Current,
                HintSurface::Editing,
            ],
        ),
        group(
            &[(Key::Home, CursorMove::Home), (Key::End, CursorMove::End)],
            "Jump",
            "Jump to start/end",
            &[HintSurface::Current, HintSurface::Editing],
        ),
    ];
    (rows, navigation)
}

#[derive(Clone, Copy)]
enum KeyModifiers {
    None,
    Shift,
    Any,
}

fn text_field_with(target: InputTarget, modifiers: KeyModifiers) -> Vec<Row> {
    let mut rows = readline_rows(target);
    rows.extend(deletion_rows(target, modifiers));
    rows.extend(cursor_rows(target, modifiers));
    rows
}

fn deletion_rows(target: InputTarget, modifiers: KeyModifiers) -> Vec<Row> {
    vec![
        hidden(Action::TextBackspace { target }, Key::Backspace, modifiers),
        hidden(Action::TextDelete { target }, Key::Delete, modifiers),
    ]
}

fn cursor_rows(target: InputTarget, modifiers: KeyModifiers) -> Vec<Row> {
    vec![
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
    ]
}

pub(super) fn list_navigation(target: ListTarget, vim_keys: bool) -> Row {
    let mut next = vec![KeyCombo::plain(Key::Down), KeyCombo::ctrl(Key::Char('n'))];
    let mut previous = vec![KeyCombo::plain(Key::Up), KeyCombo::ctrl(Key::Char('p'))];
    if vim_keys {
        next.insert(0, KeyCombo::plain(Key::Char('j')));
        previous.insert(0, KeyCombo::plain(Key::Char('k')));
    }
    Row::new(
        vec![
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
        "Navigate",
        "Navigate",
        &[HintSurface::Current],
    )
}

fn hidden(action: Action, key: Key, modifiers: KeyModifiers) -> Row {
    let combos = [KeyCombo::plain(key), KeyCombo::shift(key)];
    let binding = match modifiers {
        KeyModifiers::None => Binding::action(action, &combos[..1]),
        KeyModifiers::Shift => Binding::action(action, &combos),
        KeyModifiers::Any => Binding::any_modifiers(action, key),
    };
    Row::new(vec![binding], "", "", &[])
}

fn hidden_combo(action: Action, combo: KeyCombo) -> Row {
    hidden_binding(action, &[combo])
}

fn hidden_binding(action: Action, combos: &[KeyCombo]) -> Row {
    let description = match &action {
        Action::TextMoveCursor {
            direction: CursorMove::LineStart,
            ..
        } => "Move to line start",
        Action::TextMoveCursor {
            direction: CursorMove::LineEnd,
            ..
        } => "Move to line end",
        Action::TextMoveCursor {
            direction: CursorMove::Left,
            ..
        } => "Move left",
        Action::TextMoveCursor {
            direction: CursorMove::Right,
            ..
        } => "Move right",
        Action::TextMoveCursor {
            direction: CursorMove::ReadlineWordStart,
            ..
        } => "Move to previous word",
        Action::TextMoveCursor {
            direction: CursorMove::ReadlineWordEnd,
            ..
        } => "Move to next word",
        Action::TextBackspace { .. } => "Delete previous character",
        Action::TextDelete { .. } => "Delete next character",
        Action::TextKill {
            direction: TextKillDirection::ToLineEnd,
            ..
        } => "Cut to line end",
        Action::TextKill {
            direction: TextKillDirection::ToLineStart,
            ..
        } => "Cut to line start",
        Action::TextKill {
            direction: TextKillDirection::ReadlinePreviousWhitespace,
            ..
        } => "Cut previous word",
        Action::TextKill {
            direction: TextKillDirection::ReadlineWordEnd,
            ..
        } => "Cut next word",
        Action::TextYank { .. } => "Paste cut text",
        _ => unreachable!(),
    };
    Row {
        bindings: vec![Binding::action(action, combos)],
        short: "",
        description,
        references: &[HintSurface::Editing],
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
