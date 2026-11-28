use crate::model::app_state::AppState;
use crate::model::browse::inspector_view_model::InspectorViewModel;
use crate::model::shared::viewport::{calculate_next_column_offset, calculate_prev_column_offset};
use crate::services::AppServices;
use crate::update::action::{ScrollAmount, ScrollDirection};

use super::inspector_max_scroll;

fn inspector_page_scroll_delta(
    view_model: &InspectorViewModel,
    pane_height: u16,
    amount: ScrollAmount,
) -> Option<usize> {
    amount.page_delta(view_model.visible_rows(pane_height))
}

pub(in crate::update) fn scroll_inspector(
    state: &mut AppState,
    direction: ScrollDirection,
    amount: ScrollAmount,
    services: &AppServices,
) {
    match (direction, amount) {
        (ScrollDirection::Up | ScrollDirection::Down, ScrollAmount::Line) => {
            let max = inspector_max_scroll(state, services);
            state.ui.inspector_scroll_offset =
                direction.clamp_vertical_offset(state.ui.inspector_scroll_offset, max, 1);
        }
        (ScrollDirection::Up, ScrollAmount::ToStart) => {
            state.ui.inspector_scroll_offset = 0;
        }
        (ScrollDirection::Down, ScrollAmount::ToEnd) => {
            state.ui.inspector_scroll_offset = inspector_max_scroll(state, services);
        }
        (_, ScrollAmount::HalfPage | ScrollAmount::FullPage) => {
            let view_model = state.inspector_view_model(services.ddl_generator.as_ref());
            if let Some(delta) =
                inspector_page_scroll_delta(&view_model, state.ui.inspector_pane_height, amount)
            {
                let max = view_model.max_scroll(state.ui.inspector_pane_height);
                state.ui.inspector_scroll_offset =
                    direction.clamp_vertical_offset(state.ui.inspector_scroll_offset, max, delta);
            }
        }
        (ScrollDirection::Left, ScrollAmount::Line) => {
            state.ui.inspector_horizontal_offset =
                calculate_prev_column_offset(state.ui.inspector_horizontal_offset);
        }
        (ScrollDirection::Right, ScrollAmount::Line) => {
            state.ui.inspector_horizontal_offset = calculate_next_column_offset(
                state.ui.inspector_horizontal_offset,
                state.ui.inspector_viewport_plan.max_offset,
            );
        }
        _ => debug_assert!(
            false,
            "unsupported Inspector scroll: {direction:?} {amount:?}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, ColumnAttributes, ConnectionId, Table};
    use crate::model::shared::inspector_tab::InspectorTab;
    use crate::test_support::endpoint;
    use crate::update::action::{Action, ScrollTarget};
    use crate::update::reducer::route;
    use std::time::Instant;

    mod inspector_scroll_top_bottom {
        use crate::test_support;

        use super::*;

        fn state_with_table_detail(columns: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;
            state.ui.inspector_tab = InspectorTab::Columns;
            state.session.activate_connection(
                &ConnectionId::new(),
                "postgres",
                &endpoint::postgres("postgres://test"),
            );
            let cols: Vec<Column> = (0..columns)
                .map(|i| Column {
                    attributes: ColumnAttributes::empty(),
                    ..test_support::column::test_nullable_column(
                        format!("col_{i}"),
                        "text",
                        i as i32,
                    )
                })
                .collect();
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "test_table".to_string(),
                columns: cols,
                row_count_estimate: Some(0),
                ..test_support::table::minimal("", "")
            }));
            state
        }

        #[test]
        fn inspector_scroll_top_resets_to_zero() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 10;

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ToStart,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn inspector_scroll_bottom_goes_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 0;
            let services = AppServices::stub();
            let expected_max = state
                .inspector_view_model(services.ddl_generator.as_ref())
                .max_scroll(state.ui.inspector_pane_height);

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                Instant::now(),
                &services,
            );

            assert_eq!(state.ui.inspector_scroll_offset, expected_max);
        }

        #[test]
        fn inspector_scroll_bottom_no_detail_stays_zero() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_pane_height = 10;

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn inspector_half_page_scroll_advances_by_half_visible_rows() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 1;

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_scroll_offset, 3);
        }

        #[test]
        fn inspector_full_page_scroll_clamps_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.inspector_scroll_offset = 12;

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_scroll_offset, 15);
        }

        #[test]
        fn inspector_page_scroll_stays_zero_when_content_fits_viewport() {
            let mut state = state_with_table_detail(4);

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        fn use_sqlite_tabs(state: &mut AppState) {
            state.session.activate_connection(
                &ConnectionId::new(),
                "sqlite",
                &endpoint::sqlite("sqlite://test.db"),
            );
        }

        mod info_tab {
            use super::*;

            #[test]
            fn postgresql_uses_all_fields() {
                let mut state = state_with_table_detail(0);
                state.ui.inspector_pane_height = 8;
                state.ui.inspector_tab = InspectorTab::Info;
                let services = AppServices::stub();
                let expected_max = state
                    .inspector_view_model(services.ddl_generator.as_ref())
                    .max_scroll(state.ui.inspector_pane_height);

                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::Inspector,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::ToEnd,
                    },
                    Instant::now(),
                    &services,
                );

                assert_eq!(state.ui.inspector_scroll_offset, expected_max);
            }

            #[test]
            fn sqlite_uses_supported_fields() {
                let mut state = state_with_table_detail(0);
                use_sqlite_tabs(&mut state);
                state.ui.inspector_pane_height = 7;
                state.ui.inspector_tab = InspectorTab::Info;
                let services = AppServices::stub();
                let expected_max = state
                    .inspector_view_model(services.ddl_generator.as_ref())
                    .max_scroll(state.ui.inspector_pane_height);

                route(
                    &mut state,
                    Action::Scroll {
                        target: ScrollTarget::Inspector,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::ToEnd,
                    },
                    Instant::now(),
                    &services,
                );

                assert_eq!(state.ui.inspector_scroll_offset, expected_max);
            }
        }

        #[test]
        fn inspector_half_page_scroll_uses_sqlite_ddl_tab() {
            let mut state = state_with_table_detail(20);
            use_sqlite_tabs(&mut state);
            state.ui.inspector_pane_height = 7;
            state.ui.inspector_tab = InspectorTab::Ddl;
            state.ui.inspector_scroll_offset = 1;

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[test]
        fn inspector_full_page_scroll_uses_sqlite_ddl_tab() {
            let mut state = state_with_table_detail(20);
            use_sqlite_tabs(&mut state);
            state.ui.inspector_pane_height = 7;
            state.ui.inspector_tab = InspectorTab::Ddl;
            state.ui.inspector_scroll_offset = 1;

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }

        #[rstest::rstest]
        #[case(ScrollAmount::HalfPage)]
        #[case(ScrollAmount::FullPage)]
        fn page_scroll_normalizes_unsupported_ddl_tab(#[case] amount: ScrollAmount) {
            let mut state = state_with_table_detail(20);
            state.session.clear_connection();
            state.ui.inspector_pane_height = 7;
            state.ui.inspector_tab = InspectorTab::Ddl;
            state.ui.inspector_scroll_offset = 1;

            route(
                &mut state,
                Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount,
                },
                Instant::now(),
                &AppServices::stub(),
            );

            assert_eq!(state.ui.inspector_scroll_offset, 0);
        }
    }
}
