use std::time::{Duration, Instant};

use crate::app::cmd::effect::Effect;
use crate::app::model::app_state::AppState;
use crate::app::model::shared::inspector_tab::InspectorTab;
use crate::app::model::shared::ui_state::YankFlash;
use crate::app::ports::ClipboardError;
use crate::app::services::AppServices;
use crate::app::update::action::Action;

pub fn reduce(
    state: &mut AppState,
    action: &Action,
    services: &AppServices,
    now: Instant,
) -> Option<Vec<Effect>> {
    match action {
        Action::ResultCellYank => {
            if let (Some(row_idx), Some(col_idx)) = (
                state.result_interaction.selection().row(),
                state.result_interaction.selection().cell(),
            ) {
                let content = state
                    .query
                    .visible_result()
                    .and_then(|r| r.rows.get(row_idx))
                    .and_then(|row| row.get(col_idx))
                    .cloned();
                if let Some(value) = content {
                    state.result_interaction.yank_flash = Some(YankFlash {
                        row: row_idx,
                        col: Some(col_idx),
                        until: now + Duration::from_millis(200),
                    });
                    Some(vec![Effect::CopyToClipboard {
                        content: value,
                        on_success: Some(Action::CellCopied),
                        on_failure: Some(clipboard_unavailable()),
                    }])
                } else {
                    state
                        .messages
                        .set_error_at("Cell index out of bounds".into(), now);
                    Some(vec![])
                }
            } else {
                Some(vec![])
            }
        }
        Action::ResultRowYankOperatorPending => {
            state.result_interaction.yank_op_pending = true;
            Some(vec![])
        }
        Action::DdlYank => {
            if state.ui.inspector_tab == InspectorTab::Ddl
                && let Some(table) = state.session.table_detail().as_ref()
            {
                let ddl = services.ddl_generator.generate_ddl(table);
                state
                    .flash_timers
                    .set(crate::app::model::shared::flash_timer::FlashId::Ddl, now);
                return Some(vec![Effect::CopyToClipboard {
                    content: ddl,
                    on_success: Some(Action::CellCopied),
                    on_failure: Some(Action::CopyFailed(crate::app::ports::ClipboardError {
                        message: "Clipboard unavailable".into(),
                    })),
                }]);
            }
            Some(vec![])
        }
        Action::ResultRowYank => {
            if let Some(row_idx) = state.result_interaction.selection().row() {
                let content = state
                    .query
                    .visible_result()
                    .and_then(|r| r.rows.get(row_idx))
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
                    state.result_interaction.yank_flash = Some(YankFlash {
                        row: row_idx,
                        col: None,
                        until: now + Duration::from_millis(200),
                    });
                    Some(vec![Effect::CopyToClipboard {
                        content: tsv,
                        on_success: Some(Action::CellCopied),
                        on_failure: Some(clipboard_unavailable()),
                    }])
                } else {
                    state
                        .messages
                        .set_error_at("Row index out of bounds".into(), now);
                    Some(vec![])
                }
            } else {
                Some(vec![])
            }
        }
        Action::CellCopied => Some(vec![]),
        Action::CopyFailed(e) => {
            state.messages.set_error_at(e.to_string(), now);
            Some(vec![])
        }
        _ => None,
    }
}

fn clipboard_unavailable() -> Action {
    Action::CopyFailed(ClipboardError {
        message: "Clipboard unavailable".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ports::ddl_generator::DdlGenerator;
    use crate::domain::{Column, Table};
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
            let row_count = result_rows.len();
            state
                .query
                .set_current_result(Arc::new(crate::domain::QueryResult {
                    query: String::new(),
                    columns,
                    rows: result_rows,
                    row_count,
                    execution_time_ms: 1,
                    executed_at: Instant::now(),
                    source: crate::domain::QuerySource::Preview,
                    error: None,
                    command_tag: None,
                }));
            state
        }

        #[test]
        fn out_of_bounds_row_sets_error() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(10, 0);

            let effects = reduce(
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

            let effects = reduce(
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

            let effects = reduce(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => {
                    assert_eq!(content, "r1c2");
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
        }

        #[test]
        fn row_yank_pending_does_not_copy_cell() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(1, 2);

            let effects = reduce(
                &mut state,
                &Action::ResultRowYankOperatorPending,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(effects.is_empty());
            assert!(state.result_interaction.yank_op_pending);
        }

        #[test]
        fn history_mode_yanks_visible_cell() {
            let mut state = state_with_grid(1, 1);
            state
                .query
                .push_history(Arc::new(crate::domain::QueryResult::success(
                    String::new(),
                    vec!["col_0".to_string()],
                    vec![vec!["history".to_string()]],
                    1,
                    crate::domain::QuerySource::Adhoc,
                )));
            state.query.enter_history(0);
            state.result_interaction.activate_cell(0, 0);

            let effects = reduce(
                &mut state,
                &Action::ResultCellYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => assert_eq!(content, "history"),
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
        }

        #[test]
        fn no_cell_selection_is_noop() {
            let mut state = state_with_grid(3, 3);

            let effects = reduce(
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
                .set_current_result(Arc::new(crate::domain::QueryResult {
                    query: String::new(),
                    columns,
                    rows,
                    row_count: 1,
                    execution_time_ms: 1,
                    executed_at: Instant::now(),
                    source: crate::domain::QuerySource::Preview,
                    error: None,
                    command_tag: None,
                }));
            state
        }

        #[test]
        fn emits_tsv_copy_effect() {
            let mut state = state_with_row(vec!["v0", "v1", "v2"]);
            state.result_interaction.activate_cell(0, 0);

            let effects = reduce(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => {
                    assert_eq!(content, "v0\tv1\tv2");
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
        }

        #[test]
        fn history_mode_yanks_visible_row() {
            let mut state = state_with_row(vec!["live"]);
            state
                .query
                .push_history(Arc::new(crate::domain::QueryResult::success(
                    String::new(),
                    vec!["col_0".to_string()],
                    vec![vec!["history".to_string()]],
                    1,
                    crate::domain::QuerySource::Adhoc,
                )));
            state.query.enter_history(0);
            state.result_interaction.activate_cell(0, 0);

            let effects = reduce(
                &mut state,
                &Action::ResultRowYank,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            match &effects[0] {
                Effect::CopyToClipboard { content, .. } => assert_eq!(content, "history"),
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
        }

        #[test]
        fn escapes_tab_and_newline() {
            let mut state = state_with_row(vec!["a\tb", "c\nd"]);
            state.result_interaction.activate_cell(0, 0);

            let effects = reduce(
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

            let effects = reduce(
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

            let effects = reduce(
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

            let effects = reduce(
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
            fn generate_ddl(&self, table: &Table) -> String {
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
            state.ui.inspector_tab = InspectorTab::Ddl;
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                owner: None,
                columns: vec![Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default: None,
                    is_primary_key: true,
                    is_unique: true,
                    comment: None,
                    ordinal_position: 1,
                }],
                primary_key: Some(vec!["id".to_string()]),
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
        fn with_table_detail_returns_copy_effect() {
            let mut state = state_with_ddl_tab();

            let effects = reduce(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.expect("should return Some");
            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], Effect::CopyToClipboard { content, .. } if content.contains("CREATE TABLE"))
            );
        }

        #[test]
        fn sets_flash() {
            let mut state = state_with_ddl_tab();
            let now = Instant::now();

            reduce(&mut state, &Action::DdlYank, &fake_services(), now);

            assert!(
                state
                    .flash_timers
                    .is_active(crate::app::model::shared::flash_timer::FlashId::Ddl, now)
            );
        }

        #[test]
        fn without_table_detail_returns_empty() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_tab = InspectorTab::Ddl;

            let effects = reduce(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.expect("should return Some");
            assert!(effects.is_empty());
        }

        #[test]
        fn on_non_ddl_tab_returns_empty() {
            let mut state = state_with_ddl_tab();
            state.ui.inspector_tab = InspectorTab::Info;

            let effects = reduce(
                &mut state,
                &Action::DdlYank,
                &fake_services(),
                Instant::now(),
            );

            let effects = effects.expect("should return Some");
            assert!(effects.is_empty());
        }
    }
}
