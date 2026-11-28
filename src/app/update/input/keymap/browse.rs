use std::sync::LazyLock;

use crate::model::shared::focused_pane::FocusedPane;
use crate::model::shared::key_sequence::Prefix;
use crate::ports::inbound::{Key, KeyCombo};
use crate::update::action::{
    Action, CursorPosition, ModalKind, ScrollAmount, ScrollDirection, ScrollTarget,
    ScrollToCursorTarget, SelectMotion,
};

use super::common::{NavigationKey, NavigationLayout, navigation_binding};
use super::context::BrowsePane;
use super::global;
use super::model::{Binding, Row, hints, indexed_hints, row_action};
use super::{Hint, HintSurface, KeyContext};

pub(super) static EXPLORER: LazyLock<Vec<Row>> = LazyLock::new(|| rows(BrowsePane::Explorer));
pub(super) static INSPECTOR: LazyLock<Vec<Row>> = LazyLock::new(|| rows(BrowsePane::Inspector));
pub(super) static RESULT_SCROLL: LazyLock<Vec<Row>> =
    LazyLock::new(|| rows(BrowsePane::ResultScroll));
pub(super) static RESULT_CELL: LazyLock<Vec<Row>> = LazyLock::new(|| rows(BrowsePane::ResultCell));

fn rows(pane: BrowsePane) -> Vec<Row> {
    let mut rows = global::ROWS.clone();
    rows.push({
        let mut row = Row {
            visible: Some(|context, surface| {
                surface != HintSurface::FooterAction
                    || (!context.browse_focused && context.browse_pane != BrowsePane::ResultCell)
            }),
            ..row(
                vec![
                    Binding::action(Action::ConfirmSelection, &[KeyCombo::plain(Key::Enter)])
                        .when(|context| context.browse_connection_error),
                ],
                "Error",
                "Open connection error details",
                &[HintSurface::Current, HintSurface::FooterAction],
            )
        };
        row.surfaces.extend_from_slice(&[HintSurface::Connections]);
        row
    });
    let mut pane_switch = Vec::new();
    for (key, target) in [
        ('1', FocusedPane::Explorer),
        ('2', FocusedPane::Inspector),
        ('3', FocusedPane::Result),
    ] {
        pane_switch.push(
            Binding::any_modifiers(Action::ToggleFocus, Key::Char(key))
                .when(|context| context.browse_focused),
        );
        pane_switch.push(
            Binding::any_modifiers(Action::SetFocusedPane(target), Key::Char(key))
                .when(|context| !context.browse_focused),
        );
    }
    rows.push({
        let mut row = row(
            pane_switch,
            "Pane",
            "Switch pane focus",
            &[HintSurface::Current],
        );
        row.surfaces.extend_from_slice(&[HintSurface::OpenSwitch]);
        row
    });
    for key in ['n', 'p'] {
        rows.push(row(
            vec![Binding::with_ctrl(Action::None, Key::Char(key)).consume()],
            "",
            "",
            &[],
        ));
    }

    rows.extend(navigation(pane));
    match pane {
        BrowsePane::Explorer => {
            rows.push(row(
                vec![Binding::no_ctrl_alt(Action::ConfirmSelection, Key::Enter)],
                "Select",
                "Select table",
                &[HintSurface::Current, HintSurface::Navigation],
            ));
            rows.push(row(
                vec![Binding::no_ctrl_alt(Action::Escape, Key::Esc)],
                "Back",
                "Cancel",
                &[HintSurface::Current, HintSurface::Navigation],
            ));
        }
        BrowsePane::Inspector => {
            rows.push(row(
                vec![Binding::no_ctrl_alt(Action::Escape, Key::Esc)],
                "Back",
                "Cancel",
                &[HintSurface::Current, HintSurface::Navigation],
            ));
            rows.push({
                let mut row = Row {
                    visible: Some(|context, surface| {
                        surface != HintSurface::FooterAction
                            || (!context.browse_focused
                                && context.browse_pane != BrowsePane::ResultCell)
                    }),
                    ..row(
                        vec![
                            Binding::no_ctrl_alt(Action::DdlYank, Key::Char('y'))
                                .when(|context| context.browse_inspector_ddl),
                        ],
                        "Yank",
                        "Copy DDL to clipboard",
                        &[HintSurface::Current, HintSurface::FooterAction],
                    )
                };
                row.surfaces.extend_from_slice(&[HintSurface::DataActions]);
                row
            });
            rows.push({
                let mut row = Row {
                    visible: Some(|context, surface| {
                        surface != HintSurface::FooterNavigation
                            || context.browse_multiple_inspector_tabs
                    }),
                    ..row(
                        vec![
                            Binding::any_modifiers(Action::InspectorNextTab, Key::Tab),
                            Binding::any_modifiers(Action::InspectorPrevTab, Key::BackTab),
                        ],
                        "InsTabs",
                        "Inspector prev/next tab",
                        &[HintSurface::Current, HintSurface::FooterNavigation],
                    )
                };
                row.surfaces.extend_from_slice(&[HintSurface::OpenSwitch]);
                row
            });
        }
        BrowsePane::ResultScroll | BrowsePane::ResultCell => {
            if pane == BrowsePane::ResultScroll {
                rows.push(row(
                    vec![Binding::no_ctrl_alt(Action::ResultActivateCell, Key::Enter)],
                    "Select",
                    "Enter cell selection at the current viewport anchor",
                    &[
                        HintSurface::Current,
                        HintSurface::FooterAction,
                        HintSurface::Navigation,
                    ],
                ));
                rows.push({
                    let mut row = Row {
                        visible: Some(|context, surface| {
                            surface != HintSurface::FooterClose || context.browse_staged_delete
                        }),
                        dynamic_label: Some(|context| {
                            if context.browse_staged_delete {
                                ("Clear Del", "Clear all staged row deletions")
                            } else {
                                ("Back", "Cancel")
                            }
                        }),
                        ..row(
                            vec![
                                Binding::no_ctrl_alt(Action::ClearStagedDeletes, Key::Esc)
                                    .when(|context| context.browse_staged_delete),
                                Binding::no_ctrl_alt(Action::Escape, Key::Esc)
                                    .when(|context| !context.browse_staged_delete),
                            ],
                            "Back",
                            "Cancel",
                            &[HintSurface::Current, HintSurface::FooterClose],
                        )
                    };
                    row.surfaces.extend_from_slice(&[HintSurface::DataActions]);
                    row
                });
                rows.push(row(
                    vec![Binding::no_ctrl_alt(Action::None, Key::Char('y')).consume()],
                    "",
                    "",
                    &[],
                ));
            } else {
                rows.push({
                    let mut row = Row {
                        visible: Some(|context, surface| {
                            !matches!(surface, HintSurface::FooterAction | HintSurface::Current)
                                || (!context.browse_staged_delete && !context.browse_draft)
                        }),
                        ..row(
                            vec![Binding::no_ctrl_alt(
                                Action::ResultOpenCellDetail,
                                Key::Enter,
                            )],
                            "Detail",
                            "Open active cell detail",
                            &[HintSurface::Current, HintSurface::FooterAction],
                        )
                    };
                    row.surfaces.extend_from_slice(&[HintSurface::DataActions]);
                    row
                });
                rows.push({
                    let mut row = Row {
                        dynamic_label: Some(|context| {
                            if context.browse_draft {
                                ("Discard", "Discard pending draft")
                            } else {
                                ("Back", "Exit cell selection")
                            }
                        }),
                        ..row(
                            vec![
                                Binding::no_ctrl_alt(Action::ResultDiscardCellEdit, Key::Esc)
                                    .when(|context| context.browse_draft),
                                Binding::no_ctrl_alt(Action::ResultExitToScroll, Key::Esc)
                                    .when(|context| !context.browse_draft),
                            ],
                            "Back",
                            "Exit cell selection or discard draft",
                            &[HintSurface::Current, HintSurface::FooterClose],
                        )
                    };
                    row.surfaces.extend_from_slice(&[HintSurface::Editing]);
                    row
                });
                rows.push({
                    let mut row = Row {
                        visible: Some(|context, surface| {
                            !matches!(surface, HintSurface::FooterAction | HintSurface::Current)
                                || (!context.browse_staged_delete
                                    && (context.browse_can_edit_cell || context.browse_draft))
                        }),
                        ..row(
                            vec![Binding::no_ctrl_alt(
                                Action::ResultEnterCellEdit,
                                Key::Char('i'),
                            )],
                            "Edit",
                            "Edit active cell",
                            &[HintSurface::Current, HintSurface::FooterAction],
                        )
                    };
                    row.surfaces.extend_from_slice(&[HintSurface::Editing]);
                    row
                });
                rows.push({
                    let mut row = Row {
                        visible: Some(|context, surface| {
                            !matches!(surface, HintSurface::FooterAction | HintSurface::Current)
                                || (!context.browse_staged_delete && !context.browse_draft)
                        }),
                        ..row(
                            vec![
                                Binding::repeat(
                                    Action::ResultRowYankOperatorPending,
                                    Key::Char('y'),
                                )
                                .when(|context| !context.browse_yank_pending),
                                Binding::repeat(Action::ResultRowYank, Key::Char('y'))
                                    .when(|context| context.browse_yank_pending),
                            ],
                            "Yank Row",
                            "Copy the active row values to clipboard (TSV)",
                            &[HintSurface::Current, HintSurface::FooterAction],
                        )
                    };
                    row.surfaces.extend_from_slice(&[HintSurface::DataActions]);
                    row
                });
                rows.push({
                    let mut row = Row {
                        visible: Some(|context, surface| {
                            !matches!(surface, HintSurface::FooterAction | HintSurface::Current)
                                || (!context.browse_draft
                                    && (context.browse_staged_delete || context.browse_can_write))
                        }),
                        ..row(
                            vec![
                                Binding::repeat(
                                    Action::ResultDeleteOperatorPending,
                                    Key::Char('d'),
                                )
                                .when(|context| !context.browse_delete_pending),
                                Binding::repeat(Action::StageRowForDelete, Key::Char('d'))
                                    .when(|context| context.browse_delete_pending),
                            ],
                            "Stage Del",
                            "Stage the active row for deletion",
                            &[HintSurface::Current, HintSurface::FooterAction],
                        )
                    };
                    row.surfaces.extend_from_slice(&[HintSurface::DataActions]);
                    row
                });
            }
            rows.push({
                let mut row = Row {
                    visible: Some(|context, surface| {
                        !matches!(surface, HintSurface::FooterAction | HintSurface::Current)
                            || !context.browse_draft
                    }),
                    ..row(
                        vec![
                            Binding::action(
                                Action::OpenModal(ModalKind::RowDetail),
                                &[KeyCombo::plain(Key::Char('K'))],
                            )
                            .when(|context| {
                                !context.browse_staged_delete && context.browse_selected_row
                            }),
                        ],
                        "Row Detail",
                        "Open Row Detail",
                        &[HintSurface::Current, HintSurface::FooterAction],
                    )
                };
                row.surfaces.extend_from_slice(&[HintSurface::DataActions]);
                row
            });
            rows.push({
                let mut row = Row {
                    visible: Some(|context, surface| {
                        !matches!(surface, HintSurface::FooterAction | HintSurface::Current)
                            || !context.browse_draft
                    }),
                    ..row(
                        vec![
                            Binding::action(
                                Action::ResultCellYank,
                                &[KeyCombo::plain(Key::Char('Y'))],
                            )
                            .when(|context| {
                                !context.browse_staged_delete && context.browse_selected_cell
                            }),
                        ],
                        "Yank Cell",
                        "Copy the active cell value to clipboard",
                        &[HintSurface::Current, HintSurface::FooterAction],
                    )
                };
                row.surfaces.extend_from_slice(&[HintSurface::DataActions]);
                row
            });
            rows.push({
                let mut row = Row {
                    visible: Some(|context, surface| {
                        !matches!(surface, HintSurface::FooterAction | HintSurface::Current)
                            || !context.browse_draft
                    }),
                    ..row(
                        vec![
                            Binding::any_modifiers(Action::UnstageLastStagedRow, Key::Char('u'))
                                .when(|context| context.browse_staged_delete),
                        ],
                        "Unstage",
                        "Unstage the last staged row deletion",
                        &[HintSurface::Current, HintSurface::FooterAction],
                    )
                };
                row.surfaces.extend_from_slice(&[HintSurface::DataActions]);
                row
            });
            rows.push(Row {
                visible: Some(|context, surface| {
                    surface != HintSurface::FooterNavigation
                        || (context.browse_pane == BrowsePane::ResultScroll
                            && context.browse_can_paginate)
                }),
                ..row(
                    vec![
                        Binding::any_modifiers(Action::ResultNextPage, Key::Char(']')),
                        Binding::any_modifiers(Action::ResultPrevPage, Key::Char('[')),
                    ],
                    "Page",
                    "Next/Previous page",
                    &[
                        HintSurface::Current,
                        HintSurface::FooterNavigation,
                        HintSurface::Navigation,
                    ],
                )
            });
        }
    }
    rows
}

fn row(
    bindings: Vec<Binding>,
    short: &'static str,
    description: &'static str,
    surfaces: &'static [HintSurface],
) -> Row {
    Row::new(bindings, short, description, surfaces)
}

#[derive(Clone, Copy)]
enum Movement {
    Down,
    Up,
    First,
    Last,
    Top,
    Middle,
    Bottom,
    Left,
    Right,
    HalfDown,
    HalfUp,
    PageDown,
    PageUp,
    CenterCursor,
    TopCursor,
    BottomCursor,
}

fn navigation(pane: BrowsePane) -> Vec<Row> {
    use Movement::{
        Bottom, BottomCursor, CenterCursor, Down, First, HalfDown, HalfUp, Last, Left, Middle,
        PageDown, PageUp, Right, Top, TopCursor, Up,
    };
    let bind = |movement, combos: &[KeyCombo]| {
        Binding::action(
            navigation_action(pane, movement).unwrap_or(Action::None),
            combos,
        )
    };
    let navigate = |movement, key| {
        navigation_binding(
            NavigationLayout::Browse,
            key,
            navigation_action(pane, movement).unwrap_or(Action::None),
        )
    };
    let seq = |movement, key| {
        Binding::sequence(
            navigation_action(pane, movement).unwrap_or(Action::CancelKeySequence),
            Prefix::Z,
            KeyCombo::plain(Key::Char(key)),
            false,
        )
        .with_any_sequence_start()
    };
    let navigation = &[HintSurface::Navigation];
    vec![
        row(
            vec![
                navigate(Down, NavigationKey::Down),
                navigate(Up, NavigationKey::Up),
            ],
            "Scroll",
            "Move down/up",
            navigation,
        ),
        row(
            vec![
                navigate(Left, NavigationKey::Left),
                navigate(Right, NavigationKey::Right),
            ],
            "Horizontal",
            "Move left/right",
            navigation,
        ),
        row(
            vec![
                navigate(First, NavigationKey::First),
                navigate(Last, NavigationKey::Last),
            ],
            "Top/Bottom",
            "First/last item",
            navigation,
        ),
        row(
            vec![
                navigate(Top, NavigationKey::Top),
                navigate(Middle, NavigationKey::Middle),
                navigate(Bottom, NavigationKey::Bottom),
            ],
            "Viewport",
            "First/middle/last visible item",
            if pane == BrowsePane::Inspector {
                &[]
            } else {
                navigation
            },
        ),
        row(
            vec![
                bind(HalfDown, &[KeyCombo::ctrl(Key::Char('d'))]),
                bind(HalfUp, &[KeyCombo::ctrl(Key::Char('u'))]),
            ],
            "Half Page",
            "Scroll half page down/up",
            navigation,
        ),
        row(
            vec![
                bind(
                    PageDown,
                    &[
                        KeyCombo::ctrl(Key::Char('f')),
                        KeyCombo::plain(Key::PageDown),
                    ],
                ),
                bind(
                    PageUp,
                    &[KeyCombo::ctrl(Key::Char('b')), KeyCombo::plain(Key::PageUp)],
                ),
            ],
            "Full Page",
            "Scroll full page down/up",
            navigation,
        ),
        row(
            vec![
                seq(CenterCursor, 'z'),
                seq(TopCursor, 't'),
                seq(BottomCursor, 'b'),
            ],
            "Scroll To",
            "Scroll cursor to center/top/bottom",
            if pane == BrowsePane::Inspector {
                &[]
            } else {
                navigation
            },
        ),
    ]
}

fn navigation_action(pane: BrowsePane, movement: Movement) -> Option<Action> {
    use Movement::{
        Bottom, BottomCursor, CenterCursor, Down, First, HalfDown, HalfUp, Last, Left, Middle,
        PageDown, PageUp, Right, Top, TopCursor, Up,
    };
    if pane == BrowsePane::Explorer {
        let motion = match movement {
            Down => Some(SelectMotion::Next),
            Up => Some(SelectMotion::Previous),
            First => Some(SelectMotion::First),
            Last => Some(SelectMotion::Last),
            Top => Some(SelectMotion::ViewportTop),
            Middle => Some(SelectMotion::ViewportMiddle),
            Bottom => Some(SelectMotion::ViewportBottom),
            HalfDown => Some(SelectMotion::HalfPageDown),
            HalfUp => Some(SelectMotion::HalfPageUp),
            PageDown => Some(SelectMotion::FullPageDown),
            PageUp => Some(SelectMotion::FullPageUp),
            _ => None,
        };
        if let Some(motion) = motion {
            return Some(Action::Select(motion));
        }
    }
    if pane == BrowsePane::ResultCell {
        match movement {
            Left => return Some(Action::ResultCellLeft),
            Right => return Some(Action::ResultCellRight),
            _ => {}
        }
    }
    let target = match pane {
        BrowsePane::Explorer => ScrollTarget::Explorer,
        BrowsePane::Inspector => ScrollTarget::Inspector,
        BrowsePane::ResultScroll | BrowsePane::ResultCell => ScrollTarget::Result,
    };
    if matches!(movement, CenterCursor | TopCursor | BottomCursor) {
        let target = match pane {
            BrowsePane::Explorer => ScrollToCursorTarget::Explorer,
            BrowsePane::Inspector => return None,
            BrowsePane::ResultScroll | BrowsePane::ResultCell => ScrollToCursorTarget::Result,
        };
        let position = match movement {
            CenterCursor => CursorPosition::Center,
            TopCursor => CursorPosition::Top,
            _ => CursorPosition::Bottom,
        };
        return Some(Action::ScrollToCursor { target, position });
    }
    if pane == BrowsePane::Inspector && matches!(movement, Top | Middle | Bottom) {
        return None;
    }
    let (direction, amount) = match movement {
        Down => (ScrollDirection::Down, ScrollAmount::Line),
        Up => (ScrollDirection::Up, ScrollAmount::Line),
        Left => (ScrollDirection::Left, ScrollAmount::Line),
        Right => (ScrollDirection::Right, ScrollAmount::Line),
        First => (ScrollDirection::Up, ScrollAmount::ToStart),
        Last => (ScrollDirection::Down, ScrollAmount::ToEnd),
        Top => (ScrollDirection::Up, ScrollAmount::ViewportTop),
        Middle => (ScrollDirection::Up, ScrollAmount::ViewportMiddle),
        Bottom => (ScrollDirection::Down, ScrollAmount::ViewportBottom),
        HalfDown => (ScrollDirection::Down, ScrollAmount::HalfPage),
        HalfUp => (ScrollDirection::Up, ScrollAmount::HalfPage),
        PageDown => (ScrollDirection::Down, ScrollAmount::FullPage),
        PageUp => (ScrollDirection::Up, ScrollAmount::FullPage),
        CenterCursor | TopCursor | BottomCursor => unreachable!(),
    };
    Some(Action::Scroll {
        target,
        direction,
        amount,
    })
}

pub(super) fn active_rows(context: &KeyContext) -> &'static [Row] {
    match context.browse_pane {
        BrowsePane::Explorer => &EXPLORER,
        BrowsePane::Inspector => &INSPECTOR,
        BrowsePane::ResultScroll => &RESULT_SCROLL,
        BrowsePane::ResultCell => &RESULT_CELL,
    }
}

#[derive(Clone, Copy)]
pub enum BrowseShortcut {
    ReloadMetadata,
    SelectConnection,
    ErrorDetails,
}

pub fn browse_shortcut(shortcut: BrowseShortcut, context: &KeyContext) -> Option<String> {
    let rows = active_rows(context);
    indexed_hints(rows, context, HintSurface::Current)
        .into_iter()
        .find_map(|(index, hint)| {
            let action = row_action(&rows[index])?;
            let matches = match shortcut {
                BrowseShortcut::ReloadMetadata => matches!(action, Action::ReloadMetadata),
                BrowseShortcut::SelectConnection => {
                    matches!(action, Action::OpenModal(ModalKind::ConnectionSelector))
                }
                BrowseShortcut::ErrorDetails => {
                    matches!(action, Action::ConfirmSelection) && context.browse_connection_error
                }
            };
            matches.then_some(hint.key)
        })
}

pub fn browse_hints(surface: HintSurface, context: &KeyContext) -> Vec<Hint> {
    let rows = active_rows(context);
    if surface == HintSurface::Current {
        let (global, pane): (Vec<_>, Vec<_>) = indexed_hints(rows, context, surface)
            .into_iter()
            .partition(|(index, _)| *index < global::ROWS.len());
        let mut result: Vec<_> = pane.into_iter().map(|(_, hint)| hint).collect();
        if context.pending_prefix.is_none() {
            result.extend(super::command::browse_write_hints(context));
        }
        result.extend(global.into_iter().map(|(_, hint)| hint));
        return result;
    }
    if surface != HintSurface::Footer {
        return hints(rows, context, surface);
    }
    let mut result = Vec::new();
    result.extend(hints(rows, context, HintSurface::Footer));
    for group in [
        HintSurface::FooterAction,
        HintSurface::FooterNavigation,
        HintSurface::FooterHelp,
        HintSurface::FooterClose,
        HintSurface::FooterQuit,
    ] {
        let (global, pane): (Vec<_>, Vec<_>) = indexed_hints(rows, context, group)
            .into_iter()
            .partition(|(index, _)| *index < global::ROWS.len());
        result.extend(pane.into_iter().map(|(_, hint)| hint));
        if group == HintSurface::FooterAction && context.pending_prefix.is_none() {
            result.extend(super::command::browse_write_hints(context));
        }
        result.extend(global.into_iter().map(|(_, hint)| hint));
    }
    result
}
