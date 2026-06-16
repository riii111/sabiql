use std::time::Instant;

use crate::cmd::effect::Effect;
#[cfg(test)]
use crate::domain::ColumnAttributes;
use crate::model::app_state::AppState;
use crate::model::browse::cell_detail::{CellDetailMode, CellDetailState};
use crate::model::shared::flash_timer::FlashId;
use crate::model::shared::input_mode::InputMode;
use crate::model::shared::key_sequence::KeySequenceState;
use crate::model::shared::text_input::TextInputLike;
use crate::ports::outbound::ClipboardError;
use crate::update::action::{Action, CursorMove, InputTarget, ModalKind};
use crate::update::dispatch_result::DispatchResult;
use crate::update::helpers::EditGuardrailError;
use crate::update::helpers::find_text_matches;

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
            if let Err(reason) = apply_pending_edit_as_draft(state) {
                state.messages.set_error_at(reason.to_string(), now);
            }
            state.cell_detail.close();
            state.modal.pop_mode();
            DispatchResult::handled()
        }
        Action::CellDetailYankAll => DispatchResult::handled_with(vec![Effect::CopyToClipboard {
            content: state.cell_detail.current_content().to_string(),
            on_success: Some(Box::new(Action::CellDetailYankSuccess)),
            on_failure: Some(Box::new(Action::CopyFailed(ClipboardError::Unavailable(
                "Clipboard unavailable".into(),
            )))),
        }]),
        Action::CellDetailYankSuccess => {
            state.flash_timers.set(FlashId::CellDetail, now);
            DispatchResult::handled()
        }
        Action::CellDetailEnterEdit => {
            if let Err(reason) = ensure_cell_detail_editable(state) {
                state.messages.set_error_at(reason.to_string(), now);
                return DispatchResult::handled();
            }
            state.cell_detail.enter_edit();
            state.modal.replace_mode(InputMode::CellDetail);
            DispatchResult::handled()
        }
        Action::CellDetailAppendInsert => {
            if let Err(reason) = ensure_cell_detail_editable(state) {
                state.messages.set_error_at(reason.to_string(), now);
                return DispatchResult::handled();
            }
            state
                .cell_detail
                .editor_mut()
                .move_cursor(CursorMove::LineEnd);
            state.cell_detail.enter_edit();
            state.modal.replace_mode(InputMode::CellDetail);
            DispatchResult::handled()
        }
        Action::CellDetailExitEdit => {
            state.cell_detail.exit_edit();
            state.modal.replace_mode(InputMode::CellDetail);
            DispatchResult::handled()
        }
        Action::TextInput {
            target: InputTarget::CellDetailContent,
            ch,
        } => {
            if *ch == '\n' {
                state.cell_detail.editor_mut().insert_newline();
            } else if *ch == '\t' {
                state.cell_detail.editor_mut().insert_tab();
            } else {
                state.cell_detail.editor_mut().insert_char(*ch);
            }
            state.cell_detail.update_editor_scroll();
            DispatchResult::handled()
        }
        Action::TextBackspace {
            target: InputTarget::CellDetailContent,
        } => {
            state.cell_detail.editor_mut().backspace();
            state.cell_detail.update_editor_scroll();
            DispatchResult::handled()
        }
        Action::TextDelete {
            target: InputTarget::CellDetailContent,
        } => {
            state.cell_detail.editor_mut().delete();
            state.cell_detail.update_editor_scroll();
            DispatchResult::handled()
        }
        Action::TextMoveCursor {
            target: InputTarget::CellDetailContent,
            direction,
        } => {
            state.cell_detail.move_editor_cursor(*direction);
            state.ui.set_key_sequence(KeySequenceState::Idle);
            DispatchResult::handled()
        }
        Action::Paste(text)
            if state.input_mode() == InputMode::CellDetail
                && state.cell_detail.mode() == CellDetailMode::Editing =>
        {
            state.cell_detail.editor_mut().insert_str(text);
            state.cell_detail.update_editor_scroll();
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
    if data_type != Some("json") {
        return value.to_string();
    }

    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|json| serde_json::to_string_pretty(&json).ok())
        .unwrap_or_else(|| value.to_string())
}

fn update_search_matches(state: &mut AppState) {
    let query = state.cell_detail.search().input().content().to_string();
    let matches = find_text_matches(state.cell_detail.current_content(), &query);
    state.cell_detail.search_mut().set_matches(matches);
}

fn ensure_cell_detail_editable(state: &AppState) -> Result<String, EditGuardrailError> {
    if state.session.is_read_only() {
        return Err(EditGuardrailError::GuardrailBlocked(
            "Read-only mode: editing is disabled".to_string(),
        ));
    }

    super::edit::editable_cell_context_at(state, state.cell_detail.row(), state.cell_detail.col())
}

fn apply_pending_edit_as_draft(state: &mut AppState) -> Result<(), EditGuardrailError> {
    if !state.cell_detail.has_pending_changes() {
        return Ok(());
    }

    let row = state.cell_detail.row();
    let col = state.cell_detail.col();
    let original_cell = ensure_cell_detail_editable(state)?;
    let draft = state.cell_detail.editor().content().to_string();

    state
        .result_interaction
        .begin_cell_edit(row, col, original_cell);
    state.result_interaction.clear_write_preview();
    state
        .result_interaction
        .cell_edit_input_mut()
        .set_content(draft);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::column::Column;
    use crate::domain::{QueryResult, QuerySource, Table};
    use crate::services::AppServices;
    use crate::update::reducer::reduce as reduce_app;
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
        assert_eq!(state.cell_detail.current_content(), "short");
    }

    #[test]
    fn json_column_opens_read_only_pretty_detail() {
        let mut state = state_with_cell("json", r#"{"b":2,"a":1}"#);

        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(
            state.cell_detail.current_content(),
            "{\n  \"a\": 1,\n  \"b\": 2\n}"
        );
        assert_eq!(state.cell_detail.original_content(), r#"{"b":2,"a":1}"#);
    }

    #[test]
    fn text_json_container_keeps_original_format() {
        let mut state = state_with_cell("text", r#"{"items":["admin","writer"]}"#);

        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(
            state.cell_detail.current_content(),
            r#"{"items":["admin","writer"]}"#
        );
        assert_eq!(
            state.cell_detail.original_content(),
            r#"{"items":["admin","writer"]}"#
        );
    }

    #[test]
    fn yank_all_copies_current_detail_content() {
        let mut state = state_with_cell("json", r#"{"b":2,"a":1}"#);
        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        let result = reduce_cell_detail(&mut state, &Action::CellDetailYankAll, Instant::now());

        assert!(matches!(
            result.expect("yank should copy").as_slice(),
            [Effect::CopyToClipboard { content, .. }] if content == "{\n  \"a\": 1,\n  \"b\": 2\n}"
        ));
    }

    #[test]
    fn enter_and_exit_edit_keeps_cell_detail_modal_active() {
        let mut state = state_with_cell("text", "hello");
        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        reduce_cell_detail(&mut state, &Action::CellDetailEnterEdit, Instant::now());

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(state.cell_detail.mode(), CellDetailMode::Editing);

        reduce_cell_detail(&mut state, &Action::CellDetailExitEdit, Instant::now());

        assert_eq!(state.input_mode(), InputMode::CellDetail);
        assert_eq!(state.cell_detail.mode(), CellDetailMode::Viewing);
    }

    #[test]
    fn close_after_edit_keeps_draft_inline() {
        let mut state = state_with_cell("text", "hello");
        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());
        reduce_cell_detail(&mut state, &Action::CellDetailEnterEdit, Instant::now());
        state
            .cell_detail
            .editor_mut()
            .set_content("hello world".to_string());
        reduce_cell_detail(&mut state, &Action::CellDetailExitEdit, Instant::now());

        reduce_cell_detail(
            &mut state,
            &Action::CloseModal(ModalKind::CellDetail),
            Instant::now(),
        );

        assert_eq!(state.input_mode(), InputMode::Normal);
        assert_eq!(
            state.result_interaction.cell_edit().draft_value(),
            "hello world"
        );
    }

    #[test]
    fn close_without_changes_does_not_start_inline_edit() {
        let mut state = state_with_cell("text", "hello");
        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());
        reduce_cell_detail(&mut state, &Action::CellDetailEnterEdit, Instant::now());
        reduce_cell_detail(&mut state, &Action::CellDetailExitEdit, Instant::now());

        reduce_cell_detail(
            &mut state,
            &Action::CloseModal(ModalKind::CellDetail),
            Instant::now(),
        );

        assert!(!state.result_interaction.cell_edit().is_active());
    }

    #[test]
    fn primary_key_cell_detail_cannot_enter_edit() {
        let mut state = state_with_cell("integer", "1");
        state.result_interaction.activate_cell(0, 0);
        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());

        reduce_cell_detail(&mut state, &Action::CellDetailEnterEdit, Instant::now());

        assert_eq!(state.cell_detail.mode(), CellDetailMode::Viewing);
        assert_eq!(
            state.messages.last_error.as_deref(),
            Some("Primary key columns are read-only")
        );
    }

    #[test]
    fn close_does_not_create_draft_when_detail_cell_is_not_editable() {
        let mut state = state_with_cell("integer", "1");
        state.result_interaction.activate_cell(0, 0);
        reduce_cell_detail(&mut state, &Action::ResultOpenCellDetail, Instant::now());
        state.cell_detail.editor_mut().set_content("2".to_string());

        reduce_cell_detail(
            &mut state,
            &Action::CloseModal(ModalKind::CellDetail),
            Instant::now(),
        );

        assert!(!state.result_interaction.cell_edit().is_active());
        assert_eq!(
            state.messages.last_error.as_deref(),
            Some("Primary key columns are read-only")
        );
    }

    #[test]
    fn close_after_edit_can_continue_to_write_preview() {
        let mut state = state_with_cell("text", "hello");
        let services = AppServices::stub();
        let now = Instant::now();

        reduce_app(&mut state, Action::ResultOpenCellDetail, now, &services);
        reduce_app(&mut state, Action::CellDetailEnterEdit, now, &services);
        state
            .cell_detail
            .editor_mut()
            .set_content("updated".to_string());
        reduce_app(&mut state, Action::CellDetailExitEdit, now, &services);
        reduce_app(
            &mut state,
            Action::CloseModal(ModalKind::CellDetail),
            now,
            &services,
        );

        assert!(!state.cell_detail.is_active());
        assert_eq!(
            state.result_interaction.cell_edit().draft_value(),
            "updated"
        );

        let effects = reduce_app(&mut state, Action::SubmitCellEditWrite, now, &services);
        let preview = match effects.first() {
            Some(Effect::DispatchActions(actions)) => match actions.first() {
                Some(Action::OpenWritePreviewConfirm(preview)) => preview.clone(),
                other => panic!("expected OpenWritePreviewConfirm, got {other:?}"),
            },
            other => panic!("expected DispatchActions, got {other:?}"),
        };

        reduce_app(
            &mut state,
            Action::OpenWritePreviewConfirm(preview),
            now,
            &services,
        );

        assert_eq!(state.input_mode(), InputMode::ConfirmDialog);
        assert!(state.result_interaction.pending_write_preview().is_some());
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
