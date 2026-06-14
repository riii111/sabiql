use std::time::Instant;

use unicode_casefold::UnicodeCaseFold;

use crate::cmd::effect::Effect;
#[cfg(test)]
use crate::domain::ColumnAttributes;
use crate::model::app_state::AppState;
use crate::model::browse::cell_detail::CellDetailState;
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::ports::outbound::ClipboardError;
use crate::update::action::{Action, InputTarget, ModalKind, ScrollDirection, ScrollTarget};
use crate::update::dispatch_result::DispatchResult;

pub fn reduce_cell_detail(state: &mut AppState, action: &Action, now: Instant) -> DispatchResult {
    match action {
        Action::ResultOpenCellDetail => {
            if selected_column_is_jsonb(state) {
                return DispatchResult::handled_with(vec![Effect::DispatchActions(vec![
                    Action::OpenModal(ModalKind::JsonbDetail),
                ])]);
            }

            let Some((row_idx, col_idx, column_name, cell_value, data_type)) =
                selected_cell_value(state)
            else {
                return DispatchResult::handled();
            };

            let display_value = cell_detail_display_value(&cell_value, data_type.as_deref());
            state.cell_detail =
                CellDetailState::open(row_idx, col_idx, column_name, cell_value, display_value);
            state.modal.push_mode(InputMode::CellDetail);
            DispatchResult::handled()
        }
        Action::CloseModal(ModalKind::CellDetail) => {
            state.cell_detail.close();
            state.modal.pop_mode();
            DispatchResult::handled()
        }
        Action::CellDetailYankAll => DispatchResult::handled_with(vec![Effect::CopyToClipboard {
            content: state.cell_detail.original_content().to_string(),
            on_success: Some(Box::new(Action::CellDetailYankSuccess)),
            on_failure: Some(Box::new(Action::CopyFailed(ClipboardError::Unavailable(
                "Clipboard unavailable".into(),
            )))),
        }]),
        Action::CellDetailYankSuccess => {
            state.flash_timers.set(FlashId::CellDetail, now);
            DispatchResult::handled()
        }
        Action::CellDetailEnterSearch => {
            state.cell_detail.enter_search();
            DispatchResult::handled()
        }
        Action::CellDetailExitSearch => {
            state.cell_detail.exit_search();
            DispatchResult::handled()
        }
        Action::CellDetailSearchSubmit => {
            state.cell_detail.exit_search();
            state.cell_detail.scroll_to_match();
            DispatchResult::handled()
        }
        Action::CellDetailSearchNext => {
            state.cell_detail.search_mut().advance_to_next_match();
            state.cell_detail.scroll_to_match();
            DispatchResult::handled()
        }
        Action::CellDetailSearchPrev => {
            state.cell_detail.search_mut().advance_to_prev_match();
            state.cell_detail.scroll_to_match();
            DispatchResult::handled()
        }
        Action::TextInput {
            target: InputTarget::CellDetailSearch,
            ch,
        } => {
            state.cell_detail.search_mut().input_mut().insert_char(*ch);
            update_search_matches(state);
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::CellDetailSearch,
        } => {
            state.cell_detail.search_mut().input_mut().backspace();
            update_search_matches(state);
            DispatchResult::handled()
        }
        Action::TextDelete {
            target: InputTarget::CellDetailSearch,
        } => {
            state.cell_detail.search_mut().input_mut().delete();
            update_search_matches(state);
            DispatchResult::handled()
        }
        Action::TextMoveCursor {
            target: InputTarget::CellDetailSearch,
            direction,
        } => {
            state
                .cell_detail
                .search_mut()
                .input_mut()
                .move_cursor(*direction);
            DispatchResult::handled()
        }
        Action::Paste(text)
            if state.input_mode() == InputMode::CellDetail
                && state.cell_detail.search().is_active() =>
        {
            let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
            state
                .cell_detail
                .search_mut()
                .input_mut()
                .insert_str(&clean);
            update_search_matches(state);
            DispatchResult::handled()
        }
        Action::Scroll {
            target: ScrollTarget::CellDetail,
            direction: direction @ (ScrollDirection::Down | ScrollDirection::Up),
            amount,
        } => {
            state.cell_detail.scroll(*direction, *amount);
            DispatchResult::handled()
        }
        _ => DispatchResult::pass(),
    }
}

fn selected_cell_value(state: &AppState) -> Option<(usize, usize, String, String, Option<String>)> {
    let result = state.query.visible_result().filter(|r| !r.is_error())?;
    let row_idx = state.result_interaction.selection().row()?;
    let col_idx = state.result_interaction.selection().cell()?;
    let column_name = result.columns.get(col_idx)?.clone();
    let cell_value = result.rows.get(row_idx)?.get(col_idx)?.clone();
    let data_type = selected_column_data_type(state, col_idx).map(ToString::to_string);
    Some((row_idx, col_idx, column_name, cell_value, data_type))
}

fn selected_column_is_jsonb(state: &AppState) -> bool {
    let Some(col_idx) = state.result_interaction.selection().cell() else {
        return false;
    };
    selected_column_data_type(state, col_idx).is_some_and(|data_type| data_type == "jsonb")
}

fn selected_column_data_type(state: &AppState, col_idx: usize) -> Option<&str> {
    let td = state.session.table_detail()?;
    if td.schema != state.query.pagination.schema() || td.name != state.query.pagination.table() {
        return None;
    }
    td.columns
        .get(col_idx)
        .map(|column| column.data_type.as_str())
}

fn cell_detail_display_value(value: &str, data_type: Option<&str>) -> String {
    let should_pretty_print_json = data_type == Some("json") || looks_like_json_container(value);
    if !should_pretty_print_json {
        return value.to_string();
    }

    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|json| serde_json::to_string_pretty(&json).ok())
        .unwrap_or_else(|| value.to_string())
}

fn looks_like_json_container(value: &str) -> bool {
    let trimmed = value.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn update_search_matches(state: &mut AppState) {
    let query = state.cell_detail.search().input().content().to_string();
    let matches = find_text_matches(state.cell_detail.content(), &query);
    state.cell_detail.search_mut().set_matches(matches);
}

fn find_text_matches(content: &str, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_folded = query.case_fold().collect::<String>();
    let mut matches = Vec::new();
    let mut offset = 0;

    for segment in content.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let (folded, offset_map) = casefold_with_char_offsets(line);
        let mut search_from = 0;
        while let Some(rel_idx) = folded[search_from..].find(&query_folded) {
            let match_idx = search_from + rel_idx;
            matches.push(offset + original_char_offset_for_folded_byte(&offset_map, match_idx));
            search_from = match_idx + query_folded.len();
        }
        offset += segment.chars().count();
    }

    matches
}

fn casefold_with_char_offsets(text: &str) -> (String, Vec<(usize, usize)>) {
    let mut folded = String::new();
    let mut offset_map = Vec::new();

    for (original_char_offset, ch) in text.chars().enumerate() {
        for folded_char in ch.case_fold() {
            offset_map.push((folded.len(), original_char_offset));
            folded.push(folded_char);
        }
    }

    offset_map.push((folded.len(), text.chars().count()));
    (folded, offset_map)
}

fn original_char_offset_for_folded_byte(
    offset_map: &[(usize, usize)],
    folded_byte_offset: usize,
) -> usize {
    let idx = offset_map.partition_point(|(byte_offset, _)| *byte_offset <= folded_byte_offset);
    offset_map[idx.saturating_sub(1)].1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::column::Column;
    use crate::domain::{QueryResult, QuerySource, Table};
    use std::sync::Arc;

    fn state_with_cell(data_type: &str, cell_value: &str) -> AppState {
        let mut state = AppState::new("test".to_string());
        state
            .query
            .set_current_result(Arc::new(QueryResult::success(
                String::new(),
                vec!["id".to_string(), "body".to_string()],
                vec![vec!["1".to_string(), cell_value.to_string()]],
                1,
                QuerySource::Preview,
            )));
        state.query.pagination.reset_for_table("public", "notes");
        state.session.set_table_detail_raw(Some(Table {
            schema: "public".to_string(),
            name: "notes".to_string(),
            owner: None,
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    default: None,
                    attributes: ColumnAttributes::PRIMARY_KEY,
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "body".to_string(),
                    data_type: data_type.to_string(),
                    default: None,
                    attributes: ColumnAttributes::NULLABLE,
                    comment: None,
                    ordinal_position: 2,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            indexes: vec![],
            rls: None,
            triggers: vec![],
            row_count_estimate: None,
            comment: None,
        }));
        state.result_interaction.activate_cell(0, 1);
        state
    }

    #[test]
    fn long_text_cell_opens_read_only_detail() {
        let mut state = state_with_cell("text", &"a".repeat(60));

        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert!(state.cell_detail.is_active());
        assert_eq!(state.cell_detail.column_name(), "body");
    }

    #[test]
    fn short_text_cell_opens_detail() {
        let mut state = state_with_cell("text", "short");

        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(state.cell_detail.content(), "short");
    }

    #[test]
    fn json_column_opens_read_only_pretty_detail() {
        let mut state = state_with_cell("json", r#"{"b":2,"a":1}"#);

        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(state.cell_detail.content(), "{\n  \"a\": 1,\n  \"b\": 2\n}");
        assert_eq!(state.cell_detail.original_content(), r#"{"b":2,"a":1}"#);
    }

    #[test]
    fn text_json_container_opens_read_only_pretty_detail() {
        let mut state = state_with_cell("text", r#"{"items":["admin","writer"]}"#);

        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(
            state.cell_detail.content(),
            "{\n  \"items\": [\n    \"admin\",\n    \"writer\"\n  ]\n}"
        );
        assert_eq!(
            state.cell_detail.original_content(),
            r#"{"items":["admin","writer"]}"#
        );
    }

    #[test]
    fn yank_all_copies_original_cell_value() {
        let mut state = state_with_cell("json", r#"{"b":2,"a":1}"#);
        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        let result = reduce_cell_detail(&mut state, &Action::CellDetailYankAll, Instant::now());

        assert!(matches!(
            result.expect("yank should copy").as_slice(),
            [Effect::CopyToClipboard { content, .. }] if content == r#"{"b":2,"a":1}"#
        ));
    }

    #[test]
    fn jsonb_cell_dispatches_to_existing_jsonb_modal() {
        let mut state = state_with_cell("jsonb", r#"{"a":1}"#);

        let result = reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        assert!(matches!(
            result.expect("jsonb dispatch should be handled").as_slice(),
            [Effect::DispatchActions(actions)]
                if matches!(actions.as_slice(), [Action::OpenModal(ModalKind::JsonbDetail)])
        ));
        assert!(!state.cell_detail.is_active());
    }

    #[test]
    fn search_input_tracks_matches_case_insensitively() {
        let mut state = state_with_cell("text", "Alpha\nalpha");
        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());
        reduce_cell_detail(&mut state, &Action::CellDetailEnterSearch, Instant::now());

        reduce_cell_detail(
            &mut state,
            &Action::TextInput {
                target: InputTarget::CellDetailSearch,
                ch: 'p',
            },
            Instant::now(),
        );

        assert_eq!(state.cell_detail.search().matches(), &[2, 8]);
    }
}
