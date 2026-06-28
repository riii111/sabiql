use std::time::{Duration, Instant};

use crate::cmd::effect::Effect;
#[cfg(test)]
use crate::domain::ColumnAttributes;
use crate::model::app_state::AppState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::model::shared::ui_state::YankFlash;
use crate::ports::outbound::ClipboardError;
use crate::services::AppServices;
use crate::update::action::Action;
use crate::update::dispatch_result::DispatchResult;

pub fn reduce_yank(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> DispatchResult {
    match action {
        Action::ResultCellYank => {
            if let (Some(row_idx), Some(col_idx)) = (
                state.result_interaction.selection().row(),
                state.result_interaction.selection().cell(),
            ) {
                let content = state
                    .query
                    .visible_result()
                    .and_then(|r| r.rows().get(row_idx))
                    .and_then(|row| row.get(col_idx))
                    .cloned();
                if let Some(value) = content {
                    DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                        content: value,
                        on_success: Some(Box::new(Action::ResultCellYankSuccess {
                            row: row_idx,
                            col: col_idx,
                        })),
                        on_failure: Some(Box::new(clipboard_unavailable())),
                    }])
                } else {
                    state
                        .messages
                        .set_error_at("Cell index out of bounds".into(), now);
                    DispatchResult::handled()
                }
            } else {
                DispatchResult::handled()
            }
        }
        Action::ResultRowYankOperatorPending => {
            state.result_interaction.start_yank_operator();
            DispatchResult::handled()
        }
        Action::DdlYank => {
            if state.ui.inspector_tab() == InspectorTab::Ddl
                && let Some(table) = state.session.table_detail().as_ref()
            {
                let ddl = services
                    .ddl_generator
                    .generate_ddl(state.session.active_database_type_or_default(), table);
                return DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                    content: ddl,
                    on_success: Some(Box::new(Action::DdlYankSuccess)),
                    on_failure: Some(Box::new(clipboard_unavailable())),
                }]);
            }
            DispatchResult::handled()
        }
        Action::ResultRowYank => {
            if let Some(row_idx) = state.result_interaction.selection().row() {
                let content = state
                    .query
                    .visible_result()
                    .and_then(|r| r.rows().get(row_idx))
                    .map(|row| {
                        row.iter()
                            .map(|v| {
                                v.replace('\\', "\\\\")
                                    .replace('\t', "\\t")
                                    .replace('\n', "\\n")
                            })
                            .collect::<Vec<_>>()
                            .join("\t")
                    });
                if let Some(tsv) = content {
                    DispatchResult::handled_with(vec![Effect::CopyToClipboard {
                        content: tsv,
                        on_success: Some(Box::new(Action::ResultRowYankSuccess { row: row_idx })),
                        on_failure: Some(Box::new(clipboard_unavailable())),
                    }])
                } else {
                    state
                        .messages
                        .set_error_at("Row index out of bounds".into(), now);
                    DispatchResult::handled()
                }
            } else {
                DispatchResult::handled()
            }
        }
        Action::ResultCellYankSuccess { row, col } => {
            state.result_interaction.set_yank_flash(Some(YankFlash {
                row: *row,
                col: Some(*col),
                until: now + Duration::from_millis(200),
            }));
            DispatchResult::handled()
        }
        Action::ResultRowYankSuccess { row } => {
            state.result_interaction.set_yank_flash(Some(YankFlash {
                row: *row,
                col: None,
                until: now + Duration::from_millis(200),
            }));
            DispatchResult::handled()
        }
        Action::DdlYankSuccess => {
            state.flash_timers.set(FlashId::Ddl, now);
            DispatchResult::handled()
        }
        Action::CopyFailed(e) => {
            state.messages.set_error_at(e.to_string(), now);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}

fn clipboard_unavailable() -> Action {
    Action::CopyFailed(ClipboardError::Unavailable("Clipboard unavailable".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Table;
    use crate::ports::outbound::ddl_generator::DdlGenerator;
    use crate::test_support::column::column_fixture;
    use std::sync::Arc;

    mod cell_yank {
        use super::*;

        fn state_with_grid(rows: usize, cols: usize) -> AppState {
            let mut state = AppState::new("test".to_string());
            let columns: Vec<String> = (0..cols).map(|c| format!("col_{c}")).collect();
            let result_rows: Vec<Vec<String>> = (0..rows)
                .map(|r| {
                    let row_prefix = format!("r{r}");
                    (0..cols).map(|c| format!("{row_prefix}c{c}")).collect()
                })
                .collect();
            state
                .query
                .set_current_result(Arc::new(crate::domain::QueryResult::success(
                    String::new(),
                    columns,
                    result_rows,
                    1,
                    crate::domain::QuerySource::Preview,
                )));
            state
        }

        #[test]
        fn out_of_bounds_row_sets_error() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(10, 0);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn out_of_bounds_col_sets_error() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(0, 10);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn valid_cell_emits_copy_effect() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(1, 2);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard {
                    content,
                    on_success,
                    ..
                } => {
                    assert_eq!(content, "r1c2");
                    assert!(matches!(
                        on_success.as_deref(),
                        Some(Action::ResultCellYankSuccess { row: 1, col: 2 })
                    ));
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
            assert!(state.result_interaction.yank_flash().is_none());
        }

        #[test]
        fn success_sets_cell_flash() {
            let mut state = state_with_grid(3, 3);
            let now = Instant::now();

            reduce_yank(
                &mut state,
                &Action::ResultCellYankSuccess { row: 1, col: 2 },
                &AppServices::stub(),
                now,
            );

            let flash = state.result_interaction.yank_flash().expect("flash set");
            assert_eq!(flash.row, 1);
            assert_eq!(flash.col, Some(2));
        }

        #[test]
        fn row_yank_pending_does_not_copy_cell() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(1, 2);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultRowYankOperatorPending,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.result_interaction.is_yank_operator_pending());
        }

        #[test]
        fn no_cell_selection_is_noop() {
            let mut state = state_with_grid(3, 3);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_none());
        }
    }

    mod row_yank {
        use super::*;

        fn state_with_row(values: Vec<&str>) -> AppState {
            let mut state = AppState::new("test".to_string());
            let columns: Vec<String> = (0..values.len()).map(|c| format!("col_{c}")).collect();
            let rows = vec![values.iter().map(ToString::to_string).collect()];
            state
                .query
                .set_current_result(Arc::new(crate::domain::QueryResult::success(
                    String::new(),
                    columns,
                    rows,
                    1,
                    crate::domain::QuerySource::Preview,
                )));
            state
        }

        #[test]
        fn emits_tsv_copy_effect() {
            let mut state = state_with_row(vec!["v0", "v1", "v2"]);
            state.result_interaction.activate_cell(0, 0);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard {
                    content,
                    on_success,
                    ..
                } => {
                    assert_eq!(content, "v0\tv1\tv2");
                    assert!(matches!(
                        on_success.as_deref(),
                        Some(Action::ResultRowYankSuccess { row: 0 })
                    ));
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
            assert!(state.result_interaction.yank_flash().is_none());
        }

        #[test]
        fn success_sets_row_flash() {
            let mut state = state_with_row(vec!["v0", "v1"]);
            let now = Instant::now();

            reduce_yank(
                &mut state,
                &Action::ResultRowYankSuccess { row: 0 },
                &AppServices::stub(),
                now,
            );

            let flash = state.result_interaction.yank_flash().expect("flash set");
            assert_eq!(flash.row, 0);
            assert_eq!(flash.col, None);
        }

        #[test]
        fn escapes_tab_and_newline() {
            let mut state = state_with_row(vec!["a\tb", "c\nd"]);
            state.result_interaction.activate_cell(0, 0);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => {
                    assert_eq!(content, "a\\tb\tc\\nd");
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
        }

        #[test]
        fn escapes_backslash() {
            let mut state = state_with_row(vec!["a\\b"]);
            state.result_interaction.activate_cell(0, 0);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => {
                    assert_eq!(content, "a\\\\b");
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
        }

        #[test]
        fn out_of_bounds_sets_error() {
            let mut state = state_with_row(vec!["val"]);
            state.result_interaction.activate_cell(99, 0);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn no_row_selection_is_noop() {
            let mut state = state_with_row(vec!["val"]);

            let effects = reduce_yank(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
        }
    }

    mod ddl_yank {
        use super::*;

        struct FakeDdlGenerator;
        impl DdlGenerator for FakeDdlGenerator {
            fn generate_ddl(
                &self,
                _database_type: crate::domain::DatabaseType,
                table: &Table,
            ) -> String {
                format!("CREATE TABLE {}.{} ();", table.schema, table.name)
            }
        }

        fn fake_services() -> AppServices {
            let mut services = AppServices::stub();
            services.ddl_generator = Arc::new(FakeDdlGenerator);
            services
        }

        fn state_with_ddl_tab() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.set_inspector_tab(InspectorTab::Ddl);
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                columns: vec![column_fixture(|c| {
                    c.name = "id".into();
                    c.data_type = "integer".into();
                    c.ordinal_position = 1;
                    c.attributes = ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE;
                })],
                primary_key: Some(vec!["id".to_string()]),
                row_count_estimate: Some(0),
                ..crate::test_support::table::minimal("", "")
            }));
            state
        }

        #[test]
        fn with_table_detail_returns_copy_effect() {
            let mut state = state_with_ddl_tab();

            let effects = reduce_yank(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.into_effects().expect("should return Some");
            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard {
                    content,
                    on_success,
                    ..
                } => {
                    assert!(content.contains("CREATE TABLE"));
                    assert!(matches!(
                        on_success.as_deref(),
                        Some(Action::DdlYankSuccess)
                    ));
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
        }

        #[test]
        fn success_sets_flash() {
            let mut state = state_with_ddl_tab();
            let now = Instant::now();

            reduce_yank(&mut state, &Action::DdlYankSuccess, &fake_services(), now);

            assert!(state.flash_timers.is_active(FlashId::Ddl, now));
        }

        #[test]
        fn without_table_detail_returns_empty() {
            let mut state = AppState::new("test".to_string());
            state.ui.set_inspector_tab(InspectorTab::Ddl);

            let effects = reduce_yank(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.into_effects().expect("should return Some");
            assert!(effects.is_empty());
        }

        #[test]
        fn on_non_ddl_tab_returns_empty() {
            let mut state = state_with_ddl_tab();
            state.ui.set_inspector_tab(InspectorTab::Info);

            let effects = reduce_yank(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.into_effects().expect("should return Some");
            assert!(effects.is_empty());
        }
    }
}
