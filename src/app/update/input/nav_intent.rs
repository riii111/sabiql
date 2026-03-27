use crate::app::model::app_state::AppState;
use crate::app::model::shared::focused_pane::FocusedPane;
use crate::app::model::shared::ui_state::ResultNavMode;
use crate::app::update::action::{
    Action, CursorPosition, ScrollAmount, ScrollDirection, ScrollTarget, ScrollToCursorTarget,
    SelectMotion,
};
use crate::app::update::input::keybindings::{Key, KeyCombo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationContext {
    Explorer,
    Inspector,
    ResultScroll,
    ResultRowActive,
    ResultCellActive,
}

impl NavigationContext {
    pub fn is_result(self) -> bool {
        matches!(
            self,
            Self::ResultScroll | Self::ResultRowActive | Self::ResultCellActive
        )
    }

    pub fn is_inspector(self) -> bool {
        self == Self::Inspector
    }

    pub fn from_state(state: &AppState) -> Self {
        let result_nav = state.ui.focus_mode || state.ui.focused_pane == FocusedPane::Result;
        if result_nav {
            match state.result_interaction.selection().mode() {
                ResultNavMode::CellActive => Self::ResultCellActive,
                ResultNavMode::RowActive => Self::ResultRowActive,
                ResultNavMode::Scroll => Self::ResultScroll,
            }
        } else if state.ui.focused_pane == FocusedPane::Inspector {
            Self::Inspector
        } else {
            Self::Explorer
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavIntent {
    MoveDown,
    MoveUp,
    MoveToFirst,
    MoveToLast,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
    MoveLeft,
    MoveRight,
    HalfPageDown,
    HalfPageUp,
    FullPageDown,
    FullPageUp,
    ScrollCursorCenter,
    ScrollCursorTop,
    ScrollCursorBottom,
}

pub fn map_nav_intent(combo: &KeyCombo) -> Option<NavIntent> {
    if combo.modifiers.alt || combo.modifiers.shift {
        return None;
    }

    if combo.modifiers.ctrl {
        return match combo.key {
            Key::Char('d') => Some(NavIntent::HalfPageDown),
            Key::Char('u') => Some(NavIntent::HalfPageUp),
            Key::Char('f') => Some(NavIntent::FullPageDown),
            Key::Char('b') => Some(NavIntent::FullPageUp),
            _ => None,
        };
    }

    match combo.key {
        Key::Char('j') | Key::Down => Some(NavIntent::MoveDown),
        Key::Char('k') | Key::Up => Some(NavIntent::MoveUp),
        Key::Char('g') | Key::Home => Some(NavIntent::MoveToFirst),
        Key::Char('G') | Key::End => Some(NavIntent::MoveToLast),
        Key::Char('H') => Some(NavIntent::ViewportTop),
        Key::Char('M') => Some(NavIntent::ViewportMiddle),
        Key::Char('L') => Some(NavIntent::ViewportBottom),
        Key::Char('h') | Key::Left => Some(NavIntent::MoveLeft),
        Key::Char('l') | Key::Right => Some(NavIntent::MoveRight),
        Key::PageDown => Some(NavIntent::FullPageDown),
        Key::PageUp => Some(NavIntent::FullPageUp),
        _ => None,
    }
}

pub fn resolve(intent: NavIntent, ctx: NavigationContext) -> Action {
    use NavIntent::{
        FullPageDown, FullPageUp, HalfPageDown, HalfPageUp, MoveDown, MoveLeft, MoveRight,
        MoveToFirst, MoveToLast, MoveUp, ScrollCursorBottom, ScrollCursorCenter, ScrollCursorTop,
        ViewportBottom, ViewportMiddle, ViewportTop,
    };
    use NavigationContext::{Explorer, Inspector, ResultCellActive, ResultRowActive, ResultScroll};

    match (intent, ctx) {
        // MoveDown
        (MoveDown, Explorer) => Action::Select(SelectMotion::Next),
        (MoveDown, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        },
        (MoveDown, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        },

        // MoveUp
        (MoveUp, Explorer) => Action::Select(SelectMotion::Previous),
        (MoveUp, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        },
        (MoveUp, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        },

        // MoveToFirst
        (MoveToFirst, Explorer) => Action::Select(SelectMotion::First),
        (MoveToFirst, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ToStart,
        },
        (MoveToFirst, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ToStart,
        },

        // MoveToLast
        (MoveToLast, Explorer) => Action::Select(SelectMotion::Last),
        (MoveToLast, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::ToEnd,
        },
        (MoveToLast, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::ToEnd,
        },

        // ViewportTop
        (ViewportTop, Explorer) => Action::Select(SelectMotion::ViewportTop),
        (ViewportTop, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ViewportTop,
        },

        // ViewportMiddle
        (ViewportMiddle, Explorer) => Action::Select(SelectMotion::ViewportMiddle),
        (ViewportMiddle, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ViewportMiddle,
        },

        // ViewportBottom
        (ViewportBottom, Explorer) => Action::Select(SelectMotion::ViewportBottom),
        (ViewportBottom, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::ViewportBottom,
        },

        // MoveLeft
        (MoveLeft, Explorer) => Action::Scroll {
            target: ScrollTarget::Explorer,
            direction: ScrollDirection::Left,
            amount: ScrollAmount::Line,
        },
        (MoveLeft, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Left,
            amount: ScrollAmount::Line,
        },
        (MoveLeft, ResultScroll | ResultRowActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Left,
            amount: ScrollAmount::Line,
        },
        (MoveLeft, ResultCellActive) => Action::ResultCellLeft,

        // MoveRight
        (MoveRight, Explorer) => Action::Scroll {
            target: ScrollTarget::Explorer,
            direction: ScrollDirection::Right,
            amount: ScrollAmount::Line,
        },
        (MoveRight, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Right,
            amount: ScrollAmount::Line,
        },
        (MoveRight, ResultScroll | ResultRowActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Right,
            amount: ScrollAmount::Line,
        },
        (MoveRight, ResultCellActive) => Action::ResultCellRight,

        // HalfPageDown
        (HalfPageDown, Explorer) => Action::Select(SelectMotion::HalfPageDown),
        (HalfPageDown, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::HalfPage,
        },
        (HalfPageDown, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::HalfPage,
        },

        // HalfPageUp
        (HalfPageUp, Explorer) => Action::Select(SelectMotion::HalfPageUp),
        (HalfPageUp, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::HalfPage,
        },
        (HalfPageUp, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::HalfPage,
        },

        // FullPageDown
        (FullPageDown, Explorer) => Action::Select(SelectMotion::FullPageDown),
        (FullPageDown, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::FullPage,
        },
        (FullPageDown, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::FullPage,
        },

        // FullPageUp
        (FullPageUp, Explorer) => Action::Select(SelectMotion::FullPageUp),
        (FullPageUp, Inspector) => Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::FullPage,
        },
        (FullPageUp, ResultScroll | ResultRowActive | ResultCellActive) => Action::Scroll {
            target: ScrollTarget::Result,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::FullPage,
        },

        // ScrollCursorCenter
        (ScrollCursorCenter, Explorer) => Action::ScrollToCursor {
            target: ScrollToCursorTarget::Explorer,
            position: CursorPosition::Center,
        },
        (ScrollCursorCenter, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ScrollToCursor {
                target: ScrollToCursorTarget::Result,
                position: CursorPosition::Center,
            }
        }

        // ScrollCursorTop
        (ScrollCursorTop, Explorer) => Action::ScrollToCursor {
            target: ScrollToCursorTarget::Explorer,
            position: CursorPosition::Top,
        },
        (ScrollCursorTop, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ScrollToCursor {
                target: ScrollToCursorTarget::Result,
                position: CursorPosition::Top,
            }
        }

        // ScrollCursorBottom
        (ScrollCursorBottom, Explorer) => Action::ScrollToCursor {
            target: ScrollToCursorTarget::Explorer,
            position: CursorPosition::Bottom,
        },
        (ScrollCursorBottom, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ScrollToCursor {
                target: ScrollToCursorTarget::Result,
                position: CursorPosition::Bottom,
            }
        }

        (
            ViewportTop | ViewportMiddle | ViewportBottom | ScrollCursorCenter | ScrollCursorTop
            | ScrollCursorBottom,
            Inspector,
        ) => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::app_state::AppState;
    use crate::app::model::shared::focused_pane::FocusedPane;
    use crate::app::update::input::keybindings::{Key, KeyCombo};
    use rstest::rstest;

    // =========================================================================
    // NavigationContext::from_state
    // =========================================================================

    fn make_state(
        focused_pane: FocusedPane,
        focus_mode: bool,
        row: Option<usize>,
        cell: Option<usize>,
    ) -> AppState {
        let mut state = AppState::new("test".to_string());
        state.ui.focused_pane = focused_pane;
        state.ui.focus_mode = focus_mode;
        if let Some(r) = row {
            state.result_interaction.enter_row(r);
            if let Some(c) = cell {
                state.result_interaction.enter_cell(c);
            }
        }
        state
    }

    #[rstest]
    #[case(FocusedPane::Explorer, false, None, None, NavigationContext::Explorer)]
    #[case(
        FocusedPane::Inspector,
        false,
        None,
        None,
        NavigationContext::Inspector
    )]
    #[case(
        FocusedPane::Result,
        false,
        None,
        None,
        NavigationContext::ResultScroll
    )]
    #[case(
        FocusedPane::Result,
        false,
        Some(0),
        None,
        NavigationContext::ResultRowActive
    )]
    #[case(
        FocusedPane::Result,
        false,
        Some(0),
        Some(0),
        NavigationContext::ResultCellActive
    )]
    #[case(
        FocusedPane::Explorer,
        true,
        None,
        None,
        NavigationContext::ResultScroll
    )]
    #[case(
        FocusedPane::Inspector,
        true,
        None,
        None,
        NavigationContext::ResultScroll
    )]
    fn from_state_derives_correct_context(
        #[case] pane: FocusedPane,
        #[case] focus_mode: bool,
        #[case] row: Option<usize>,
        #[case] cell: Option<usize>,
        #[case] expected: NavigationContext,
    ) {
        let state = make_state(pane, focus_mode, row, cell);
        assert_eq!(NavigationContext::from_state(&state), expected);
    }

    // =========================================================================
    // map_nav_intent — positive cases
    // =========================================================================

    #[rstest]
    #[case(KeyCombo::plain(Key::Char('j')), NavIntent::MoveDown)]
    #[case(KeyCombo::plain(Key::Down), NavIntent::MoveDown)]
    #[case(KeyCombo::plain(Key::Char('k')), NavIntent::MoveUp)]
    #[case(KeyCombo::plain(Key::Up), NavIntent::MoveUp)]
    #[case(KeyCombo::plain(Key::Char('g')), NavIntent::MoveToFirst)]
    #[case(KeyCombo::plain(Key::Home), NavIntent::MoveToFirst)]
    #[case(KeyCombo::plain(Key::Char('G')), NavIntent::MoveToLast)]
    #[case(KeyCombo::plain(Key::End), NavIntent::MoveToLast)]
    #[case(KeyCombo::plain(Key::Char('H')), NavIntent::ViewportTop)]
    #[case(KeyCombo::plain(Key::Char('M')), NavIntent::ViewportMiddle)]
    #[case(KeyCombo::plain(Key::Char('L')), NavIntent::ViewportBottom)]
    #[case(KeyCombo::plain(Key::Char('h')), NavIntent::MoveLeft)]
    #[case(KeyCombo::plain(Key::Left), NavIntent::MoveLeft)]
    #[case(KeyCombo::plain(Key::Char('l')), NavIntent::MoveRight)]
    #[case(KeyCombo::plain(Key::Right), NavIntent::MoveRight)]
    #[case(KeyCombo::plain(Key::PageDown), NavIntent::FullPageDown)]
    #[case(KeyCombo::plain(Key::PageUp), NavIntent::FullPageUp)]
    #[case(KeyCombo::ctrl(Key::Char('d')), NavIntent::HalfPageDown)]
    #[case(KeyCombo::ctrl(Key::Char('u')), NavIntent::HalfPageUp)]
    #[case(KeyCombo::ctrl(Key::Char('f')), NavIntent::FullPageDown)]
    #[case(KeyCombo::ctrl(Key::Char('b')), NavIntent::FullPageUp)]
    fn map_nav_intent_positive(#[case] combo: KeyCombo, #[case] expected: NavIntent) {
        assert_eq!(map_nav_intent(&combo), Some(expected));
    }

    // =========================================================================
    // map_nav_intent — negative cases
    // =========================================================================

    #[rstest]
    #[case(KeyCombo::plain(Key::Char('q')))]
    #[case(KeyCombo::plain(Key::Char('?')))]
    #[case(KeyCombo::plain(Key::Char(':')))]
    #[case(KeyCombo::plain(Key::Char('s')))]
    #[case(KeyCombo::plain(Key::Char('y')))]
    #[case(KeyCombo::plain(Key::Char('d')))]
    #[case(KeyCombo::plain(Key::Esc))]
    #[case(KeyCombo::plain(Key::Enter))]
    #[case(KeyCombo::plain(Key::Tab))]
    #[case(KeyCombo::ctrl(Key::Char('p')))]
    #[case(KeyCombo::ctrl(Key::Char('h')))]
    #[case(KeyCombo::ctrl(Key::Char('k')))]
    #[case(KeyCombo::ctrl(Key::Char('r')))]
    #[case(KeyCombo::ctrl(Key::Char('o')))]
    #[case(KeyCombo::ctrl(Key::Char('e')))]
    #[case(KeyCombo::alt(Key::Char('j')))]
    #[case(KeyCombo::shift(Key::Char('j')))]
    // Shift guard contract: shift+uppercase is blocked at NavIntent level.
    // The key_translator normalizes these before they reach here, but
    // the guard documents the contract for any caller bypassing the translator.
    #[case(KeyCombo::shift(Key::Char('G')))]
    #[case(KeyCombo::shift(Key::Char('H')))]
    #[case(KeyCombo::shift(Key::Char('M')))]
    #[case(KeyCombo::shift(Key::Char('L')))]
    fn map_nav_intent_negative(#[case] combo: KeyCombo) {
        assert_eq!(map_nav_intent(&combo), None);
    }

    // =========================================================================
    // resolve — full 80-pattern matrix
    // =========================================================================

    use NavIntent::*;
    use NavigationContext::*;

    #[rstest]
    // MoveDown (5)
    #[case(MoveDown, Explorer, Action::Select(SelectMotion::Next))]
    #[case(MoveDown, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
    #[case(MoveDown, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
    #[case(MoveDown, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
    #[case(MoveDown, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::Line })]
    // MoveUp (5)
    #[case(MoveUp, Explorer, Action::Select(SelectMotion::Previous))]
    #[case(MoveUp, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
    #[case(MoveUp, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
    #[case(MoveUp, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
    #[case(MoveUp, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::Line })]
    // MoveToFirst (5)
    #[case(MoveToFirst, Explorer, Action::Select(SelectMotion::First))]
    #[case(MoveToFirst, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
    #[case(MoveToFirst, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
    #[case(MoveToFirst, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
    #[case(MoveToFirst, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ToStart })]
    // MoveToLast (5)
    #[case(MoveToLast, Explorer, Action::Select(SelectMotion::Last))]
    #[case(MoveToLast, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
    #[case(MoveToLast, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
    #[case(MoveToLast, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
    #[case(MoveToLast, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ToEnd })]
    // ViewportTop (5)
    #[case(ViewportTop, Explorer, Action::Select(SelectMotion::ViewportTop))]
    #[case(ViewportTop, Inspector, Action::None)]
    #[case(ViewportTop, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
    #[case(ViewportTop, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
    #[case(ViewportTop, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportTop })]
    // ViewportMiddle (5)
    #[case(ViewportMiddle, Explorer, Action::Select(SelectMotion::ViewportMiddle))]
    #[case(ViewportMiddle, Inspector, Action::None)]
    #[case(ViewportMiddle, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle })]
    #[case(ViewportMiddle, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle })]
    #[case(ViewportMiddle, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::ViewportMiddle })]
    // ViewportBottom (5)
    #[case(ViewportBottom, Explorer, Action::Select(SelectMotion::ViewportBottom))]
    #[case(ViewportBottom, Inspector, Action::None)]
    #[case(ViewportBottom, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom })]
    #[case(ViewportBottom, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom })]
    #[case(ViewportBottom, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::ViewportBottom })]
    // MoveLeft (5)
    #[case(MoveLeft, Explorer, Action::Scroll { target: ScrollTarget::Explorer, direction: ScrollDirection::Left, amount: ScrollAmount::Line })]
    #[case(MoveLeft, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Left, amount: ScrollAmount::Line })]
    #[case(MoveLeft, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Left, amount: ScrollAmount::Line })]
    #[case(MoveLeft, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Left, amount: ScrollAmount::Line })]
    #[case(MoveLeft, ResultCellActive, Action::ResultCellLeft)]
    // MoveRight (5)
    #[case(MoveRight, Explorer, Action::Scroll { target: ScrollTarget::Explorer, direction: ScrollDirection::Right, amount: ScrollAmount::Line })]
    #[case(MoveRight, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Right, amount: ScrollAmount::Line })]
    #[case(MoveRight, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Right, amount: ScrollAmount::Line })]
    #[case(MoveRight, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Right, amount: ScrollAmount::Line })]
    #[case(MoveRight, ResultCellActive, Action::ResultCellRight)]
    // HalfPageDown (5)
    #[case(HalfPageDown, Explorer, Action::Select(SelectMotion::HalfPageDown))]
    #[case(HalfPageDown, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Down, amount: ScrollAmount::HalfPage })]
    #[case(HalfPageDown, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::HalfPage })]
    #[case(HalfPageDown, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::HalfPage })]
    #[case(HalfPageDown, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::HalfPage })]
    // HalfPageUp (5)
    #[case(HalfPageUp, Explorer, Action::Select(SelectMotion::HalfPageUp))]
    #[case(HalfPageUp, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Up, amount: ScrollAmount::HalfPage })]
    #[case(HalfPageUp, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::HalfPage })]
    #[case(HalfPageUp, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::HalfPage })]
    #[case(HalfPageUp, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::HalfPage })]
    // FullPageDown (5)
    #[case(FullPageDown, Explorer, Action::Select(SelectMotion::FullPageDown))]
    #[case(FullPageDown, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Down, amount: ScrollAmount::FullPage })]
    #[case(FullPageDown, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::FullPage })]
    #[case(FullPageDown, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::FullPage })]
    #[case(FullPageDown, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Down, amount: ScrollAmount::FullPage })]
    // FullPageUp (5)
    #[case(FullPageUp, Explorer, Action::Select(SelectMotion::FullPageUp))]
    #[case(FullPageUp, Inspector, Action::Scroll { target: ScrollTarget::Inspector, direction: ScrollDirection::Up, amount: ScrollAmount::FullPage })]
    #[case(FullPageUp, ResultScroll, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::FullPage })]
    #[case(FullPageUp, ResultRowActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::FullPage })]
    #[case(FullPageUp, ResultCellActive, Action::Scroll { target: ScrollTarget::Result, direction: ScrollDirection::Up, amount: ScrollAmount::FullPage })]
    // ScrollCursorCenter (5)
    #[case(ScrollCursorCenter, Explorer, Action::ScrollToCursor { target: ScrollToCursorTarget::Explorer, position: CursorPosition::Center })]
    #[case(ScrollCursorCenter, Inspector, Action::None)]
    #[case(ScrollCursorCenter, ResultScroll, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
    #[case(ScrollCursorCenter, ResultRowActive, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
    #[case(ScrollCursorCenter, ResultCellActive, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Center })]
    // ScrollCursorTop (5)
    #[case(ScrollCursorTop, Explorer, Action::ScrollToCursor { target: ScrollToCursorTarget::Explorer, position: CursorPosition::Top })]
    #[case(ScrollCursorTop, Inspector, Action::None)]
    #[case(ScrollCursorTop, ResultScroll, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
    #[case(ScrollCursorTop, ResultRowActive, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
    #[case(ScrollCursorTop, ResultCellActive, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Top })]
    // ScrollCursorBottom (5)
    #[case(ScrollCursorBottom, Explorer, Action::ScrollToCursor { target: ScrollToCursorTarget::Explorer, position: CursorPosition::Bottom })]
    #[case(ScrollCursorBottom, Inspector, Action::None)]
    #[case(ScrollCursorBottom, ResultScroll, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
    #[case(ScrollCursorBottom, ResultRowActive, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
    #[case(ScrollCursorBottom, ResultCellActive, Action::ScrollToCursor { target: ScrollToCursorTarget::Result, position: CursorPosition::Bottom })]
    fn resolve_matrix(
        #[case] intent: NavIntent,
        #[case] ctx: NavigationContext,
        #[case] expected: Action,
    ) {
        let actual = resolve(intent, ctx);
        assert_eq!(
            format!("{actual:?}"),
            format!("{expected:?}"),
            "resolve({intent:?}, {ctx:?})"
        );
    }
}
