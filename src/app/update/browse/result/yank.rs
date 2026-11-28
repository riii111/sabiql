use crate::cmd::effect::Effect;
use crate::domain::QueryValue;
use crate::domain::ddl::generate_ddl;
use crate::model::app_state::AppState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::inspector_tab::InspectorTab;
use crate::update::action::Action;
use crate::update::action::CopyTarget;
use crate::update::helpers::clipboard_unavailable;

pub(in crate::update) fn result_cell_yank(state: &mut AppState) -> Vec<Effect> {
    if let (Some(row_idx), Some(col_idx)) = (
        state.result_interaction.selection().row(),
        state.result_interaction.selection().cell(),
    ) {
        let content = state.query.visible_result().and_then(|result| {
            if result.has_typed_values() {
                result
                    .value_at(row_idx, col_idx)
                    .map(QueryValue::copy_value)
            } else {
                result.display_value_at(row_idx, col_idx)
            }
        });
        if let Some(value) = content {
            vec![Effect::CopyToClipboard {
                content: value,
                on_success: Box::new(Action::CopySucceeded(CopyTarget::ResultCell {
                    row: row_idx,
                    col: col_idx,
                })),
                on_failure: Some(Box::new(clipboard_unavailable())),
            }]
        } else {
            state.messages.set_error("Cell index out of bounds".into());
            vec![]
        }
    } else {
        vec![]
    }
}

pub(in crate::update) fn ddl_yank(state: &AppState) -> Vec<Effect> {
    if state.ui.inspector_tab == InspectorTab::Ddl
        && let Some(table) = state.session.table_detail().as_ref()
    {
        let ddl = generate_ddl(state.session.active_database_type_or_default(), table);
        return vec![Effect::CopyToClipboard {
            content: ddl,
            on_success: Box::new(Action::CopySucceeded(CopyTarget::Panel(FlashId::Ddl))),
            on_failure: Some(Box::new(clipboard_unavailable())),
        }];
    }
    vec![]
}

pub(in crate::update) fn result_row_yank(state: &mut AppState) -> Vec<Effect> {
    if let Some(row_idx) = state.result_interaction.selection().row() {
        let content = state
            .query
            .visible_result()
            .and_then(|result| {
                if result.has_typed_values() {
                    result
                        .values()
                        .get(row_idx)
                        .map(|row| row.iter().map(QueryValue::copy_value).collect::<Vec<_>>())
                } else {
                    result.display_row_at(row_idx)
                }
            })
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
            vec![Effect::CopyToClipboard {
                content: tsv,
                on_success: Box::new(Action::CopySucceeded(CopyTarget::ResultRow {
                    row: row_idx,
                })),
                on_failure: Some(Box::new(clipboard_unavailable())),
            }]
        } else {
            state.messages.set_error("Row index out of bounds".into());
            vec![]
        }
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, ColumnAttributes, QueryResult, QuerySource, QueryValue, Table};
    use crate::update::reducer::route;
    use std::sync::Arc;
    use std::time::Instant;

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
                .set_current_result(Arc::new(QueryResult::success(
                    String::new(),
                    columns,
                    result_rows,
                    1,
                    QuerySource::Preview,
                )));
            state
        }

        #[test]
        fn out_of_bounds_row_sets_error() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(10, 0);

            let effects = route(&mut state, Action::ResultCellYank, Instant::now());

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn out_of_bounds_col_sets_error() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(0, 10);

            let effects = route(&mut state, Action::ResultCellYank, Instant::now());

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn valid_cell_emits_copy_effect() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(1, 2);

            let effects = route(&mut state, Action::ResultCellYank, Instant::now());

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard {
                    content,
                    on_success,
                    ..
                } => {
                    assert_eq!(content, "r1c2");
                    assert!(matches!(
                        on_success.as_ref(),
                        Action::CopySucceeded(CopyTarget::ResultCell { row: 1, col: 2 })
                    ));
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
            assert!(state.result_interaction.yank_flash().is_none());
        }

        #[test]
        fn typed_blob_cell_emits_lossless_copy_effect() {
            let mut state = AppState::new("test".to_string());
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    String::new(),
                    vec!["payload".to_string()],
                    vec![vec![QueryValue::Blob(vec![0xAB, 0xCD])]],
                    1,
                    QuerySource::Preview,
                )));
            state.result_interaction.activate_cell(0, 0);

            let effects = route(&mut state, Action::ResultCellYank, Instant::now());

            assert!(matches!(
                &effects[0],
                Effect::CopyToClipboard { content, .. } if content == "X'ABCD'"
            ));
        }

        #[test]
        fn row_yank_pending_does_not_copy_cell() {
            let mut state = state_with_grid(3, 3);
            state.result_interaction.activate_cell(1, 2);

            let effects = route(
                &mut state,
                Action::ResultRowYankOperatorPending,
                Instant::now(),
            );

            assert!(effects.is_empty());
            assert!(state.result_interaction.is_yank_operator_pending());
        }

        #[test]
        fn no_cell_selection_is_noop() {
            let mut state = state_with_grid(3, 3);

            let effects = route(&mut state, Action::ResultCellYank, Instant::now());

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
                .set_current_result(Arc::new(QueryResult::success(
                    String::new(),
                    columns,
                    rows,
                    1,
                    QuerySource::Preview,
                )));
            state
        }

        #[test]
        fn emits_tsv_copy_effect() {
            let mut state = state_with_row(vec!["v0", "v1", "v2"]);
            state.result_interaction.activate_cell(0, 0);

            let effects = route(&mut state, Action::ResultRowYank, Instant::now());

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard {
                    content,
                    on_success,
                    ..
                } => {
                    assert_eq!(content, "v0\tv1\tv2");
                    assert!(matches!(
                        on_success.as_ref(),
                        Action::CopySucceeded(CopyTarget::ResultRow { row: 0 })
                    ));
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
            assert!(state.result_interaction.yank_flash().is_none());
        }

        #[test]
        fn typed_blob_row_emits_lossless_tsv_copy_effect() {
            let mut state = AppState::new("test".to_string());
            state
                .query
                .set_current_result(Arc::new(QueryResult::success_with_values(
                    String::new(),
                    vec!["id".to_string(), "payload".to_string()],
                    vec![vec![
                        QueryValue::Text("1".to_string()),
                        QueryValue::Blob(vec![0xAB, 0xCD]),
                    ]],
                    1,
                    QuerySource::Preview,
                )));
            state.result_interaction.activate_cell(0, 0);

            let effects = route(&mut state, Action::ResultRowYank, Instant::now());

            assert!(matches!(
                &effects[0],
                Effect::CopyToClipboard { content, .. } if content == "1\tX'ABCD'"
            ));
        }

        #[test]
        fn escapes_tab_and_newline() {
            let mut state = state_with_row(vec!["a\tb", "c\nd"]);
            state.result_interaction.activate_cell(0, 0);

            let effects = route(&mut state, Action::ResultRowYank, Instant::now());

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

            let effects = route(&mut state, Action::ResultRowYank, Instant::now());

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

            let effects = route(&mut state, Action::ResultRowYank, Instant::now());

            assert!(effects.is_empty());
            assert!(state.messages.last_error.is_some());
        }

        #[test]
        fn no_row_selection_is_noop() {
            let mut state = state_with_row(vec!["val"]);

            let effects = route(&mut state, Action::ResultRowYank, Instant::now());

            assert!(effects.is_empty());
        }
    }

    mod ddl_yank {
        use crate::test_support;

        use super::*;

        fn state_with_ddl_tab() -> AppState {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_tab = InspectorTab::Ddl;
            state.session.set_table_detail_raw(Some(Table {
                schema: "public".to_string(),
                name: "users".to_string(),
                columns: vec![Column {
                    attributes: ColumnAttributes::PRIMARY_KEY | ColumnAttributes::UNIQUE,
                    ..test_support::column::test_nullable_column("id", "integer", 1)
                }],
                primary_key: Some(vec!["id".to_string()]),
                row_count_estimate: Some(0),
                ..test_support::table::minimal("", "")
            }));
            state
        }

        #[test]
        fn with_table_detail_returns_copy_effect() {
            let mut state = state_with_ddl_tab();

            let effects = route(&mut state, Action::DdlYank, Instant::now());

            assert_eq!(effects.len(), 1);
            match &effects[0] {
                Effect::CopyToClipboard {
                    content,
                    on_success,
                    ..
                } => {
                    assert!(content.contains("CREATE TABLE"));
                    assert!(matches!(
                        on_success.as_ref(),
                        Action::CopySucceeded(CopyTarget::Panel(FlashId::Ddl))
                    ));
                }
                other => panic!("expected CopyToClipboard, got {other:?}"),
            }
        }

        #[test]
        fn without_table_detail_returns_empty() {
            let mut state = AppState::new("test".to_string());
            state.ui.inspector_tab = InspectorTab::Ddl;

            let effects = route(&mut state, Action::DdlYank, Instant::now());

            assert!(effects.is_empty());
        }

        #[test]
        fn on_non_ddl_tab_returns_empty() {
            let mut state = state_with_ddl_tab();
            state.ui.inspector_tab = InspectorTab::Info;

            let effects = route(&mut state, Action::DdlYank, Instant::now());

            assert!(effects.is_empty());
        }
    }
}
