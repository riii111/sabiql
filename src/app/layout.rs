/// Header(1) + Footer(1) + CmdLine(1)
pub const LAYOUT_FIXED_ROWS: u16 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaneHeights {
    pub result_pane_height: u16,
    pub inspector_pane_height: u16,
}

pub fn compute_pane_heights(terminal_height: u16, focus_mode: bool) -> PaneHeights {
    let main_height = terminal_height.saturating_sub(LAYOUT_FIXED_ROWS);

    if focus_mode {
        PaneHeights {
            result_pane_height: main_height,
            inspector_pane_height: 0,
        }
    } else {
        let half = main_height / 2;
        PaneHeights {
            result_pane_height: half,
            inspector_pane_height: half,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(24, false, 10, 10)]
    #[case(24, true, 21, 0)]
    #[case(10, false, 3, 3)]
    #[case(10, true, 7, 0)]
    #[case(50, false, 23, 23)]
    #[case(50, true, 47, 0)]
    fn compute_pane_heights_returns_expected(
        #[case] terminal_height: u16,
        #[case] focus_mode: bool,
        #[case] expected_result: u16,
        #[case] expected_inspector: u16,
    ) {
        let heights = compute_pane_heights(terminal_height, focus_mode);

        assert_eq!(heights.result_pane_height, expected_result);
        assert_eq!(heights.inspector_pane_height, expected_inspector);
    }

    #[test]
    fn minimum_terminal_height_returns_zero_panes() {
        let heights = compute_pane_heights(3, false);

        assert_eq!(heights.result_pane_height, 0);
        assert_eq!(heights.inspector_pane_height, 0);
    }

    #[test]
    fn below_minimum_terminal_height_does_not_underflow() {
        let heights = compute_pane_heights(1, false);

        assert_eq!(heights.result_pane_height, 0);
        assert_eq!(heights.inspector_pane_height, 0);
    }

    #[test]
    fn odd_main_height_splits_evenly_with_truncation() {
        // terminal_height=24 -> main_height=21 -> 21/2=10 each (truncated)
        let heights = compute_pane_heights(24, false);

        assert_eq!(heights.result_pane_height, 10);
        assert_eq!(heights.inspector_pane_height, 10);
    }
}
