use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::viewport::{
    calculate_next_column_offset, calculate_prev_column_offset,
};
use crate::app::services::AppServices;
use crate::app::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};

use super::inspector_max_scroll;

fn inspector_page_scroll_delta(state: &AppState, amount: ScrollAmount) -> Option<usize> {
    let visible = match state.ui.inspector_tab {
        InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
        _ => state.inspector_visible_rows(),
    };

    amount.page_delta(visible)
}

pub fn reduce(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
) -> Option<Vec<Effect>> {
    match action {
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::Line,
        } => {
            state.ui.inspector_scroll_offset = state.ui.inspector_scroll_offset.saturating_sub(1);
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        } => {
            let max_offset = inspector_max_scroll(state, services);
            if state.ui.inspector_scroll_offset < max_offset {
                state.ui.inspector_scroll_offset += 1;
            }
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ToStart,
        } => {
            state.ui.inspector_scroll_offset = 0;
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::ToEnd,
        } => {
            state.ui.inspector_scroll_offset = inspector_max_scroll(state, services);
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction,
            amount: amount @ (ScrollAmount::HalfPage | ScrollAmount::FullPage),
        } => {
            if let Some(delta) = inspector_page_scroll_delta(state, *amount) {
                let max = inspector_max_scroll(state, services);
                state.ui.inspector_scroll_offset =
                    direction.clamp_vertical_offset(state.ui.inspector_scroll_offset, max, delta);
            }
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Left,
            amount: ScrollAmount::Line,
        } => {
            state.ui.inspector_horizontal_offset =
                calculate_prev_column_offset(state.ui.inspector_horizontal_offset);
            Some(vec![])
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Right,
            amount: ScrollAmount::Line,
        } => {
            let plan = &state.ui.inspector_viewport_plan;
            let all_widths_len = plan.max_offset + plan.column_count;
            state.ui.inspector_horizontal_offset = calculate_next_column_offset(
                all_widths_len,
                state.ui.inspector_horizontal_offset,
                plan.column_count,
            );
            Some(vec![])
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update::browse::navigation::reduce_navigation;
    use crate::domain::{Column, Table};
    use std::time::Instant;

    mod inspector_scroll_top_bottom {
        use super::*;

        fn state_with_table_detail(columns: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;
            state.ui.inspector_tab = InspectorTab::Columns;
            let cols: Vec<Column> = (0..columns)
                .map(|i| Column {
                    name: format!("col_{i}"),
                    data_type: "text".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: false,
                    is_unique: false,
                    comment: None,
                    ordinal_position: i as i32,
                })
                .collect();
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "test_table".to_string(),
                owner: None,
                columns: cols,
                primary_key: None,
                indexes: vec![],
                foreign_keys: vec![],
                rls: None,
                triggers: vec![],
                row_count_estimate: Some(0),
                comment: None,
            }));
            state
        }

        #[test]
        fn inspector_scroll_top_resets_to_zero() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 10;

            let effects = reduce_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ToStart,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn inspector_scroll_bottom_goes_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 0;
            let visible = state.inspector_visible_rows();
            let expected_max = 20_usize.saturating_sub(visible);

            let effects = reduce_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, expected_max);
        }

        #[test]
        fn inspector_scroll_bottom_no_detail_stays_zero() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;

            let effects = reduce_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn inspector_half_page_scroll_advances_by_half_visible_rows() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 1;

            let effects = reduce_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 3);
        }

        #[test]
        fn inspector_full_page_scroll_clamps_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 12;

            let effects = reduce_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 15);
        }

        #[test]
        fn inspector_page_scroll_stays_zero_when_content_fits_viewport() {
            let mut state = state_with_table_detail(4);

            let effects = reduce_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_some());
            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }
    }
}
