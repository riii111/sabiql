use crate::model::app_state::AppState;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::model::shared::viewport::{calculate_next_column_offset, calculate_prev_column_offset};
use crate::services::AppServices;
use crate::update::action::{Action, ScrollAmount, ScrollDirection, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;

use super::inspector_max_scroll;

fn inspector_page_scroll_delta(state: &AppState, amount: ScrollAmount) -> Option<usize> {
    let visible = match state
        .session
        .active_db_capabilities()
        .normalize_inspector_tab(state.ui.inspector_tab())
    {
        InspectorTab::Ddl => state.inspector_ddl_visible_rows(),
        _ => state.inspector_visible_rows(),
    };

    amount.page_delta(visible)
}

pub fn reduce_inspector(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
) -> DispatchResult {
    match action {
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: direction @ (ScrollDirection::Up | ScrollDirection::Down),
            amount: ScrollAmount::Line,
        } => {
            let max = inspector_max_scroll(state, services);
            state
                .ui
                .set_inspector_scroll_offset(direction.clamp_vertical_offset(
                    state.ui.inspector_scroll_offset(),
                    max,
                    1,
                ));
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Up,
            amount: ScrollAmount::ToStart,
        } => {
            state.ui.set_inspector_scroll_offset(0);
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Down,
            amount: ScrollAmount::ToEnd,
        } => {
            state
                .ui
                .set_inspector_scroll_offset(inspector_max_scroll(state, services));
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction,
            amount: amount @ (ScrollAmount::HalfPage | ScrollAmount::FullPage),
        } => {
            if let Some(delta) = inspector_page_scroll_delta(state, *amount) {
                let max = inspector_max_scroll(state, services);
                state
                    .ui
                    .set_inspector_scroll_offset(direction.clamp_vertical_offset(
                        state.ui.inspector_scroll_offset(),
                        max,
                        delta,
                    ));
            }
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Left,
            amount: ScrollAmount::Line,
        } => {
            state
                .ui
                .set_inspector_horizontal_offset(calculate_prev_column_offset(
                    state.ui.inspector_horizontal_offset(),
                ));
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::Inspector,
            direction: ScrollDirection::Right,
            amount: ScrollAmount::Line,
        } => {
            let plan = state.ui.inspector_viewport_plan();
            let all_widths_len = plan.max_offset + plan.column_count;
            state
                .ui
                .set_inspector_horizontal_offset(calculate_next_column_offset(
                    all_widths_len,
                    state.ui.inspector_horizontal_offset(),
                    plan.column_count,
                ));
            DispatchResult::handled()
        }

        _ => DispatchResult::pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, ColumnAttributes, ConnectionId, DatabaseType, Table};
    use crate::model::shared::db_capabilities::DbCapabilities;
    use crate::update::browse::navigation::dispatch_navigation;
    use std::time::Instant;

    mod inspector_scroll_top_bottom {
        use super::*;

        fn state_with_table_detail(columns: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.set_inspector_pane_height(10);
            state.ui.set_inspector_tab(InspectorTab::Columns);
            state.session.set_active_connection_with_dsn(
                &ConnectionId::new(),
                "postgres",
                DatabaseType::PostgreSQL,
                "postgres://test",
            );
            let cols: Vec<Column> = (0..columns)
                .map(|i| Column {
                    name: format!("col_{i}"),
                    data_type: "text".to_string(),
                    default: None,
                    attributes: ColumnAttributes::empty(),
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
            state.ui.set_inspector_scroll_offset(10);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Up,
                    amount: ScrollAmount::ToStart,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 0);
        }

        #[test]
        fn inspector_scroll_bottom_goes_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.set_inspector_scroll_offset(0);
            let visible = state.inspector_visible_rows();
            let expected_max = 20_usize.saturating_sub(visible);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), expected_max);
        }

        #[test]
        fn inspector_scroll_bottom_no_detail_stays_zero() {
            let mut state = AppState::new("test".to_string());
            state.ui.set_inspector_pane_height(10);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::ToEnd,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 0);
        }

        #[test]
        fn inspector_half_page_scroll_advances_by_half_visible_rows() {
            let mut state = state_with_table_detail(20);
            state.ui.set_inspector_scroll_offset(1);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 3);
        }

        #[test]
        fn inspector_full_page_scroll_clamps_to_max() {
            let mut state = state_with_table_detail(20);
            state.ui.set_inspector_scroll_offset(12);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 15);
        }

        #[test]
        fn inspector_page_scroll_stays_zero_when_content_fits_viewport() {
            let mut state = state_with_table_detail(4);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 0);
        }

        fn use_sqlite_tabs(state: &mut AppState) {
            state.session.set_active_connection_with_dsn(
                &ConnectionId::new(),
                "sqlite",
                DatabaseType::SQLite,
                "sqlite://test.db",
            );
        }

        mod info_tab {
            use super::*;

            #[test]
            fn postgresql_uses_all_fields() {
                let mut state = state_with_table_detail(0);
                state.ui.set_inspector_pane_height(8);
                state.ui.set_inspector_tab(InspectorTab::Info);
                let expected_max = DbCapabilities::postgres_like()
                    .inspector_info_line_count()
                    .saturating_sub(state.inspector_visible_rows());

                let effects = dispatch_navigation(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::Inspector,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::ToEnd,
                    },
                    &AppServices::stub(),
                    Instant::now(),
                );

                assert!(effects.is_handled());
                assert_eq!(state.ui.inspector_scroll_offset(), expected_max);
            }

            #[test]
            fn sqlite_uses_supported_fields() {
                let mut state = state_with_table_detail(0);
                use_sqlite_tabs(&mut state);
                state.ui.set_inspector_pane_height(7);
                state.ui.set_inspector_tab(InspectorTab::Info);
                let expected_max = DbCapabilities::sqlite_like()
                    .inspector_info_line_count()
                    .saturating_sub(state.inspector_visible_rows());

                let effects = dispatch_navigation(
                    &mut state,
                    &Action::Scroll {
                        target: ScrollTarget::Inspector,
                        direction: ScrollDirection::Down,
                        amount: ScrollAmount::ToEnd,
                    },
                    &AppServices::stub(),
                    Instant::now(),
                );

                assert!(effects.is_handled());
                assert_eq!(state.ui.inspector_scroll_offset(), expected_max);
            }
        }

        #[test]
        fn inspector_half_page_scroll_uses_sqlite_ddl_tab() {
            let mut state = state_with_table_detail(20);
            use_sqlite_tabs(&mut state);
            state.ui.set_inspector_pane_height(7);
            state.ui.set_inspector_tab(InspectorTab::Ddl);
            state.ui.set_inspector_scroll_offset(1);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 0);
        }

        #[test]
        fn inspector_full_page_scroll_uses_sqlite_ddl_tab() {
            let mut state = state_with_table_detail(20);
            use_sqlite_tabs(&mut state);
            state.ui.set_inspector_pane_height(7);
            state.ui.set_inspector_tab(InspectorTab::Ddl);
            state.ui.set_inspector_scroll_offset(1);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 0);
        }

        #[test]
        fn inspector_half_page_scroll_normalizes_unsupported_ddl_tab() {
            let mut state = state_with_table_detail(20);
            state.session.clear_connection();
            state.ui.set_inspector_pane_height(7);
            state.ui.set_inspector_tab(InspectorTab::Ddl);
            state.ui.set_inspector_scroll_offset(1);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::HalfPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 0);
        }

        #[test]
        fn inspector_full_page_scroll_normalizes_unsupported_ddl_tab() {
            let mut state = state_with_table_detail(20);
            state.session.clear_connection();
            state.ui.set_inspector_pane_height(7);
            state.ui.set_inspector_tab(InspectorTab::Ddl);
            state.ui.set_inspector_scroll_offset(1);

            let effects = dispatch_navigation(
                &mut state,
                &Action::Scroll {
                    target: ScrollTarget::Inspector,
                    direction: ScrollDirection::Down,
                    amount: ScrollAmount::FullPage,
                },
                &AppServices::stub(),
                Instant::now(),
            );

            assert!(effects.is_handled());
            assert_eq!(state.ui.inspector_scroll_offset(), 0);
        }
    }
}
