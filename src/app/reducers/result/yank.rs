use std::time::{Duration, Instant};

use crate::app::action::Action;
use crate::app::effect::Effect;
use crate::app::inspector_tab::InspectorTab;
use crate::app::services::AppServices;
use crate::app::state::AppState;
use crate::app::ui_state::YankFlash;

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
                    .current_result()
                    .and_then(|r| r.rows.get(row_idx))
                    .and_then(|row| row.get(col_idx))
                    .cloned();
                match content {
                    Some(value) => {
                        state.result_interaction.yank_flash = Some(YankFlash {
                            row: row_idx,
                            col: Some(col_idx),
                            until: now + Duration::from_millis(200),
                        });
                        Some(vec![Effect::CopyToClipboard {
                            content: value,
                            on_success: Some(Action::CellCopied),
                            on_failure: Some(Action::CopyFailed("Clipboard unavailable".into())),
                        }])
                    }
                    None => {
                        state
                            .messages
                            .set_error_at("Cell index out of bounds".into(), now);
                        Some(vec![])
                    }
                }
            } else {
                Some(vec![])
            }
        }
        Action::DdlYank => {
            if state.ui.inspector_tab == InspectorTab::Ddl
                && let Some(table) = state.session.table_detail().as_ref()
            {
                let ddl = services.ddl_generator.generate_ddl(table);
                return Some(vec![Effect::CopyToClipboard {
                    content: ddl,
                    on_success: Some(Action::CellCopied),
                    on_failure: Some(Action::CopyFailed("Clipboard unavailable".into())),
                }]);
            }
            Some(vec![])
        }
        Action::ResultRowYankOperatorPending => {
            state.result_interaction.yank_op_pending = true;
            Some(vec![])
        }
        Action::ResultRowYank => {
            if let Some(row_idx) = state.result_interaction.selection().row() {
                let content = state
                    .query
                    .current_result()
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
                match content {
                    Some(tsv) => {
                        state.result_interaction.yank_flash = Some(YankFlash {
                            row: row_idx,
                            col: None,
                            until: now + Duration::from_millis(200),
                        });
                        Some(vec![Effect::CopyToClipboard {
                            content: tsv,
                            on_success: Some(Action::CellCopied),
                            on_failure: Some(Action::CopyFailed("Clipboard unavailable".into())),
                        }])
                    }
                    None => {
                        state
                            .messages
                            .set_error_at("Row index out of bounds".into(), now);
                        Some(vec![])
                    }
                }
            } else {
                Some(vec![])
            }
        }
        Action::CellCopied => Some(vec![]),
        Action::CopyFailed(msg) => {
            state.messages.set_error_at(msg.clone(), now);
            Some(vec![])
        }
        _ => None,
    }
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
            let columns: Vec<String> = (0..cols).map(|c| format!("col_{}", c)).collect();
            let result_rows: Vec<Vec<String>> = (0..rows)
                .map(|r| (0..cols).map(|c| format!("r{}c{}", r, c)).collect())
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
            state.result_interaction.enter_row(10);
            state.result_interaction.enter_cell(0);

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
            state.result_interaction.enter_row(0);
            state.result_interaction.enter_cell(10);

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
            state.result_interaction.enter_row(1);
            state.result_interaction.enter_cell(2);

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
                other => panic!("expected CopyToClipboard, got {:?}", other),
            }
        }

        #[test]
        fn no_selection_is_noop() {
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
            let columns: Vec<String> = (0..values.len()).map(|c| format!("col_{}", c)).collect();
            let rows = vec![values.iter().map(|v| v.to_string()).collect()];
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
        fn row_yank_emits_tsv_copy_effect() {
            let mut state = state_with_row(vec!["v0", "v1", "v2"]);
            state.result_interaction.enter_row(0);

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
                other => panic!("expected CopyToClipboard, got {:?}", other),
            }
        }

        #[test]
        fn row_yank_escapes_tab_and_newline() {
            let mut state = state_with_row(vec!["a\tb", "c\nd"]);
            state.result_interaction.enter_row(0);

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
                other => panic!("expected CopyToClipboard, got {:?}", other),
            }
        }

        #[test]
        fn row_yank_escapes_backslash() {
            let mut state = state_with_row(vec!["a\\b"]);
            state.result_interaction.enter_row(0);

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
                other => panic!("expected CopyToClipboard, got {:?}", other),
            }
        }

        #[test]
        fn row_yank_out_of_bounds_sets_error() {
            let mut state = state_with_row(vec!["val"]);
            state.result_interaction.enter_row(99);

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
        fn row_yank_no_selection_is_noop() {
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

        #[test]
        fn yank_op_pending_sets_flag() {
            let mut state = AppState::new("test".to_string());

            reduce(
                &mut state,
                &Action::ResultRowYankOperatorPending,
                &AppServices::stub(),
                Instant::now(),
            )
            .unwrap();

            assert!(state.result_interaction.yank_op_pending);
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
        fn ddl_yank_with_table_detail_returns_copy_effect() {
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
        fn ddl_yank_without_table_detail_returns_empty() {
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
        fn ddl_yank_on_non_ddl_tab_returns_empty() {
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
