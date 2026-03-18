#[derive(Debug, Clone, Default)]
pub struct PickerState {
    pub selected: usize,
    pub scroll_offset: usize,
    pub pane_height: u16,
    pub filter_input: String,
}

impl PickerState {
    pub fn visible_items(&self) -> usize {
        self.pane_height as usize
    }

    pub fn set_selection(&mut self, index: usize) {
        self.scroll_offset = clamp_scroll_offset(index, self.scroll_offset, self.visible_items());
        self.selected = index;
    }

    pub fn reset(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }
}

pub fn clamp_scroll_offset(selected: usize, current_offset: usize, viewport: usize) -> usize {
    if viewport == 0 {
        return 0;
    }
    let bottom_edge = current_offset + viewport.saturating_sub(1);
    if selected > bottom_edge {
        selected - viewport.saturating_sub(1)
    } else if selected < current_offset {
        selected
    } else {
        current_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    mod visible_items {
        use super::*;

        #[test]
        fn zero_pane_height_returns_zero() {
            let state = PickerState::default();
            assert_eq!(state.visible_items(), 0);
        }

        #[test]
        fn returns_pane_height_as_usize() {
            let state = PickerState {
                pane_height: 20,
                ..Default::default()
            };
            assert_eq!(state.visible_items(), 20);
        }
    }

    mod reset {
        use super::*;

        #[test]
        fn clears_selected_and_scroll_offset() {
            let mut state = PickerState {
                selected: 5,
                scroll_offset: 3,
                pane_height: 10,
                filter_input: "hello".to_string(),
            };

            state.reset();

            assert_eq!(state.selected, 0);
            assert_eq!(state.scroll_offset, 0);
            // filter_input is intentionally preserved — caller clears it on open
            assert_eq!(state.filter_input, "hello");
        }
    }

    mod set_selection {
        use super::*;

        #[test]
        fn sets_selected_index() {
            let mut state = PickerState {
                pane_height: 10,
                ..Default::default()
            };

            state.set_selection(4);

            assert_eq!(state.selected, 4);
        }

        #[test]
        fn scroll_offset_stays_zero_when_within_viewport() {
            let mut state = PickerState {
                pane_height: 10,
                ..Default::default()
            };

            state.set_selection(5);

            assert_eq!(state.scroll_offset, 0);
        }

        #[test]
        fn scroll_offset_advances_when_selection_falls_below_viewport() {
            let mut state = PickerState {
                pane_height: 5,
                ..Default::default()
            };

            state.set_selection(7);

            // viewport = 5, selected = 7 → offset = 7 - (5-1) = 3
            assert_eq!(state.scroll_offset, 3);
            assert_eq!(state.selected, 7);
        }

        #[test]
        fn scroll_offset_retreats_when_selection_rises_above_viewport() {
            let mut state = PickerState {
                selected: 8,
                scroll_offset: 5,
                pane_height: 5,
                ..Default::default()
            };

            state.set_selection(2);

            assert_eq!(state.scroll_offset, 2);
            assert_eq!(state.selected, 2);
        }

        #[rstest]
        #[case(0, 0, 10, 0)]
        #[case(9, 0, 10, 0)]
        #[case(10, 0, 10, 1)]
        #[case(4, 3, 5, 3)]
        #[case(2, 3, 5, 2)]
        fn clamp_scroll_offset_cases(
            #[case] selected: usize,
            #[case] current_offset: usize,
            #[case] viewport: usize,
            #[case] expected_offset: usize,
        ) {
            assert_eq!(
                clamp_scroll_offset(selected, current_offset, viewport),
                expected_offset
            );
        }

        #[test]
        fn clamp_scroll_offset_zero_viewport_returns_zero() {
            assert_eq!(clamp_scroll_offset(5, 3, 0), 0);
        }
    }
}
