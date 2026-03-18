use crate::app::action::Action;
use crate::app::focused_pane::FocusedPane;
use crate::app::keybindings::{Key, KeyCombo};
use crate::app::state::AppState;
use crate::app::ui_state::ResultNavMode;

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
    use NavIntent::*;
    use NavigationContext::*;

    match (intent, ctx) {
        // MoveDown
        (MoveDown, Explorer) => Action::SelectNext,
        (MoveDown, Inspector) => Action::InspectorScrollDown,
        (MoveDown, ResultScroll | ResultRowActive | ResultCellActive) => Action::ResultScrollDown,

        // MoveUp
        (MoveUp, Explorer) => Action::SelectPrevious,
        (MoveUp, Inspector) => Action::InspectorScrollUp,
        (MoveUp, ResultScroll | ResultRowActive | ResultCellActive) => Action::ResultScrollUp,

        // MoveToFirst
        (MoveToFirst, Explorer) => Action::SelectFirst,
        (MoveToFirst, Inspector) => Action::InspectorScrollTop,
        (MoveToFirst, ResultScroll | ResultRowActive | ResultCellActive) => Action::ResultScrollTop,

        // MoveToLast
        (MoveToLast, Explorer) => Action::SelectLast,
        (MoveToLast, Inspector) => Action::InspectorScrollBottom,
        (MoveToLast, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollBottom
        }

        // ViewportTop
        (ViewportTop, Explorer) => Action::SelectViewportTop,
        (ViewportTop, Inspector) => Action::None,
        (ViewportTop, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollViewportTop
        }

        // ViewportMiddle
        (ViewportMiddle, Explorer) => Action::SelectViewportMiddle,
        (ViewportMiddle, Inspector) => Action::None,
        (ViewportMiddle, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollViewportMiddle
        }

        // ViewportBottom
        (ViewportBottom, Explorer) => Action::SelectViewportBottom,
        (ViewportBottom, Inspector) => Action::None,
        (ViewportBottom, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollViewportBottom
        }

        // MoveLeft
        (MoveLeft, Explorer) => Action::ExplorerScrollLeft,
        (MoveLeft, Inspector) => Action::InspectorScrollLeft,
        (MoveLeft, ResultScroll | ResultRowActive) => Action::ResultScrollLeft,
        (MoveLeft, ResultCellActive) => Action::ResultCellLeft,

        // MoveRight
        (MoveRight, Explorer) => Action::ExplorerScrollRight,
        (MoveRight, Inspector) => Action::InspectorScrollRight,
        (MoveRight, ResultScroll | ResultRowActive) => Action::ResultScrollRight,
        (MoveRight, ResultCellActive) => Action::ResultCellRight,

        // HalfPageDown
        (HalfPageDown, Explorer) => Action::SelectHalfPageDown,
        (HalfPageDown, Inspector) => Action::InspectorScrollHalfPageDown,
        (HalfPageDown, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollHalfPageDown
        }

        // HalfPageUp
        (HalfPageUp, Explorer) => Action::SelectHalfPageUp,
        (HalfPageUp, Inspector) => Action::InspectorScrollHalfPageUp,
        (HalfPageUp, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollHalfPageUp
        }

        // FullPageDown
        (FullPageDown, Explorer) => Action::SelectFullPageDown,
        (FullPageDown, Inspector) => Action::InspectorScrollFullPageDown,
        (FullPageDown, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollFullPageDown
        }

        // FullPageUp
        (FullPageUp, Explorer) => Action::SelectFullPageUp,
        (FullPageUp, Inspector) => Action::InspectorScrollFullPageUp,
        (FullPageUp, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollFullPageUp
        }

        // ScrollCursorCenter
        (ScrollCursorCenter, Explorer) => Action::ScrollCursorCenter,
        (ScrollCursorCenter, Inspector) => Action::None,
        (ScrollCursorCenter, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollCursorCenter
        }

        // ScrollCursorTop
        (ScrollCursorTop, Explorer) => Action::ScrollCursorTop,
        (ScrollCursorTop, Inspector) => Action::None,
        (ScrollCursorTop, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollCursorTop
        }

        // ScrollCursorBottom
        (ScrollCursorBottom, Explorer) => Action::ScrollCursorBottom,
        (ScrollCursorBottom, Inspector) => Action::None,
        (ScrollCursorBottom, ResultScroll | ResultRowActive | ResultCellActive) => {
            Action::ResultScrollCursorBottom
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::focused_pane::FocusedPane;
    use crate::app::keybindings::{Key, KeyCombo};
    use crate::app::state::AppState;
    use rstest::rstest;
    use std::mem::discriminant;

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
    #[case(MoveDown, Explorer, Action::SelectNext)]
    #[case(MoveDown, Inspector, Action::InspectorScrollDown)]
    #[case(MoveDown, ResultScroll, Action::ResultScrollDown)]
    #[case(MoveDown, ResultRowActive, Action::ResultScrollDown)]
    #[case(MoveDown, ResultCellActive, Action::ResultScrollDown)]
    // MoveUp (5)
    #[case(MoveUp, Explorer, Action::SelectPrevious)]
    #[case(MoveUp, Inspector, Action::InspectorScrollUp)]
    #[case(MoveUp, ResultScroll, Action::ResultScrollUp)]
    #[case(MoveUp, ResultRowActive, Action::ResultScrollUp)]
    #[case(MoveUp, ResultCellActive, Action::ResultScrollUp)]
    // MoveToFirst (5)
    #[case(MoveToFirst, Explorer, Action::SelectFirst)]
    #[case(MoveToFirst, Inspector, Action::InspectorScrollTop)]
    #[case(MoveToFirst, ResultScroll, Action::ResultScrollTop)]
    #[case(MoveToFirst, ResultRowActive, Action::ResultScrollTop)]
    #[case(MoveToFirst, ResultCellActive, Action::ResultScrollTop)]
    // MoveToLast (5)
    #[case(MoveToLast, Explorer, Action::SelectLast)]
    #[case(MoveToLast, Inspector, Action::InspectorScrollBottom)]
    #[case(MoveToLast, ResultScroll, Action::ResultScrollBottom)]
    #[case(MoveToLast, ResultRowActive, Action::ResultScrollBottom)]
    #[case(MoveToLast, ResultCellActive, Action::ResultScrollBottom)]
    // ViewportTop (5)
    #[case(ViewportTop, Explorer, Action::SelectViewportTop)]
    #[case(ViewportTop, Inspector, Action::None)]
    #[case(ViewportTop, ResultScroll, Action::ResultScrollViewportTop)]
    #[case(ViewportTop, ResultRowActive, Action::ResultScrollViewportTop)]
    #[case(ViewportTop, ResultCellActive, Action::ResultScrollViewportTop)]
    // ViewportMiddle (5)
    #[case(ViewportMiddle, Explorer, Action::SelectViewportMiddle)]
    #[case(ViewportMiddle, Inspector, Action::None)]
    #[case(ViewportMiddle, ResultScroll, Action::ResultScrollViewportMiddle)]
    #[case(ViewportMiddle, ResultRowActive, Action::ResultScrollViewportMiddle)]
    #[case(ViewportMiddle, ResultCellActive, Action::ResultScrollViewportMiddle)]
    // ViewportBottom (5)
    #[case(ViewportBottom, Explorer, Action::SelectViewportBottom)]
    #[case(ViewportBottom, Inspector, Action::None)]
    #[case(ViewportBottom, ResultScroll, Action::ResultScrollViewportBottom)]
    #[case(ViewportBottom, ResultRowActive, Action::ResultScrollViewportBottom)]
    #[case(ViewportBottom, ResultCellActive, Action::ResultScrollViewportBottom)]
    // MoveLeft (5)
    #[case(MoveLeft, Explorer, Action::ExplorerScrollLeft)]
    #[case(MoveLeft, Inspector, Action::InspectorScrollLeft)]
    #[case(MoveLeft, ResultScroll, Action::ResultScrollLeft)]
    #[case(MoveLeft, ResultRowActive, Action::ResultScrollLeft)]
    #[case(MoveLeft, ResultCellActive, Action::ResultCellLeft)]
    // MoveRight (5)
    #[case(MoveRight, Explorer, Action::ExplorerScrollRight)]
    #[case(MoveRight, Inspector, Action::InspectorScrollRight)]
    #[case(MoveRight, ResultScroll, Action::ResultScrollRight)]
    #[case(MoveRight, ResultRowActive, Action::ResultScrollRight)]
    #[case(MoveRight, ResultCellActive, Action::ResultCellRight)]
    // HalfPageDown (5)
    #[case(HalfPageDown, Explorer, Action::SelectHalfPageDown)]
    #[case(HalfPageDown, Inspector, Action::InspectorScrollHalfPageDown)]
    #[case(HalfPageDown, ResultScroll, Action::ResultScrollHalfPageDown)]
    #[case(HalfPageDown, ResultRowActive, Action::ResultScrollHalfPageDown)]
    #[case(HalfPageDown, ResultCellActive, Action::ResultScrollHalfPageDown)]
    // HalfPageUp (5)
    #[case(HalfPageUp, Explorer, Action::SelectHalfPageUp)]
    #[case(HalfPageUp, Inspector, Action::InspectorScrollHalfPageUp)]
    #[case(HalfPageUp, ResultScroll, Action::ResultScrollHalfPageUp)]
    #[case(HalfPageUp, ResultRowActive, Action::ResultScrollHalfPageUp)]
    #[case(HalfPageUp, ResultCellActive, Action::ResultScrollHalfPageUp)]
    // FullPageDown (5)
    #[case(FullPageDown, Explorer, Action::SelectFullPageDown)]
    #[case(FullPageDown, Inspector, Action::InspectorScrollFullPageDown)]
    #[case(FullPageDown, ResultScroll, Action::ResultScrollFullPageDown)]
    #[case(FullPageDown, ResultRowActive, Action::ResultScrollFullPageDown)]
    #[case(FullPageDown, ResultCellActive, Action::ResultScrollFullPageDown)]
    // FullPageUp (5)
    #[case(FullPageUp, Explorer, Action::SelectFullPageUp)]
    #[case(FullPageUp, Inspector, Action::InspectorScrollFullPageUp)]
    #[case(FullPageUp, ResultScroll, Action::ResultScrollFullPageUp)]
    #[case(FullPageUp, ResultRowActive, Action::ResultScrollFullPageUp)]
    #[case(FullPageUp, ResultCellActive, Action::ResultScrollFullPageUp)]
    // ScrollCursorCenter (5)
    #[case(ScrollCursorCenter, Explorer, Action::ScrollCursorCenter)]
    #[case(ScrollCursorCenter, Inspector, Action::None)]
    #[case(ScrollCursorCenter, ResultScroll, Action::ResultScrollCursorCenter)]
    #[case(ScrollCursorCenter, ResultRowActive, Action::ResultScrollCursorCenter)]
    #[case(ScrollCursorCenter, ResultCellActive, Action::ResultScrollCursorCenter)]
    // ScrollCursorTop (5)
    #[case(ScrollCursorTop, Explorer, Action::ScrollCursorTop)]
    #[case(ScrollCursorTop, Inspector, Action::None)]
    #[case(ScrollCursorTop, ResultScroll, Action::ResultScrollCursorTop)]
    #[case(ScrollCursorTop, ResultRowActive, Action::ResultScrollCursorTop)]
    #[case(ScrollCursorTop, ResultCellActive, Action::ResultScrollCursorTop)]
    // ScrollCursorBottom (5)
    #[case(ScrollCursorBottom, Explorer, Action::ScrollCursorBottom)]
    #[case(ScrollCursorBottom, Inspector, Action::None)]
    #[case(ScrollCursorBottom, ResultScroll, Action::ResultScrollCursorBottom)]
    #[case(ScrollCursorBottom, ResultRowActive, Action::ResultScrollCursorBottom)]
    #[case(ScrollCursorBottom, ResultCellActive, Action::ResultScrollCursorBottom)]
    fn resolve_matrix(
        #[case] intent: NavIntent,
        #[case] ctx: NavigationContext,
        #[case] expected: Action,
    ) {
        assert_eq!(discriminant(&resolve(intent, ctx)), discriminant(&expected));
    }
}
