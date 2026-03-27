use std::collections::BTreeSet;
use std::time::Instant;

use super::cell_edit::CellEditState;
use crate::app::model::shared::text_input::TextInputState;
use crate::app::model::shared::ui_state::{ResultSelection, YankFlash};
use crate::app::policy::write::write_guardrails::WritePreview;

// # Invariants
//
// - `selection`, `cell_edit`, `staged_delete_rows`, and `pending_write_preview`
//   must be reset together during mode transitions. Use the aggregate transition
//   API (`reset_view`, `reset_interaction`, `exit_cell_to_row`, `exit_row_to_scroll`)
//   instead of manipulating these fields individually.
//
// - After calling `exit_cell_to_row()` or `exit_row_to_scroll()`, the caller is
//   responsible for setting `input_mode` back to `Normal` if it was `CellEdit`.
#[derive(Debug, Clone, Default)]
pub struct ResultInteraction {
    pub scroll_offset: usize,
    pub horizontal_offset: usize,
    pub delete_op_pending: bool,
    pub yank_op_pending: bool,
    pub yank_flash: Option<YankFlash>,

    selection: ResultSelection,
    cell_edit: CellEditState,
    staged_delete_rows: BTreeSet<usize>,
    pending_write_preview: Option<WritePreview>,
}

impl ResultInteraction {
    pub fn selection(&self) -> &ResultSelection {
        &self.selection
    }

    pub fn cell_edit(&self) -> &CellEditState {
        &self.cell_edit
    }

    pub fn staged_delete_rows(&self) -> &BTreeSet<usize> {
        &self.staged_delete_rows
    }

    pub fn pending_write_preview(&self) -> Option<&WritePreview> {
        self.pending_write_preview.as_ref()
    }

    pub fn enter_row(&mut self, row: usize) {
        self.selection.enter_row(row);
    }

    pub fn move_row(&mut self, row: usize) {
        self.selection.move_row(row);
    }

    pub fn enter_cell(&mut self, col: usize) {
        self.selection.enter_cell(col);
    }

    pub fn clamp_selection(&mut self, max_rows: usize, max_cols: usize) {
        self.selection.clamp(max_rows, max_cols);
    }

    pub fn begin_cell_edit(&mut self, row: usize, col: usize, value: String) {
        self.cell_edit.begin(row, col, value);
    }

    pub fn cell_edit_input_mut(&mut self) -> &mut TextInputState {
        &mut self.cell_edit.input
    }

    pub fn clear_cell_edit(&mut self) {
        self.cell_edit.clear();
    }

    pub fn stage_row(&mut self, row: usize) {
        self.staged_delete_rows.insert(row);
    }

    pub fn unstage_last_row(&mut self) {
        if let Some(&last) = self.staged_delete_rows.iter().next_back() {
            self.staged_delete_rows.remove(&last);
        }
    }

    pub fn clear_staged_deletes(&mut self) {
        self.staged_delete_rows.clear();
    }

    pub fn set_write_preview(&mut self, preview: WritePreview) {
        self.pending_write_preview = Some(preview);
    }

    pub fn clear_write_preview(&mut self) {
        self.pending_write_preview = None;
    }

    pub fn discard_cell_edit(&mut self) {
        self.cell_edit.clear();
        self.pending_write_preview = None;
    }

    pub fn reset_view(&mut self) {
        self.scroll_offset = 0;
        self.horizontal_offset = 0;
        self.selection.reset();
        self.cell_edit.clear();
        self.staged_delete_rows.clear();
        self.pending_write_preview = None;
    }

    pub fn reset_interaction(&mut self) {
        self.selection.reset();
        self.cell_edit.clear();
        self.staged_delete_rows.clear();
        self.pending_write_preview = None;
    }

    // Preserves `staged_delete_rows`: cell edit is orthogonal to delete staging,
    // so returning to row-active should not discard a partially-staged batch.
    //
    // Caller must set `input_mode` to `Normal` if it was `CellEdit` (SAB-136).
    pub fn exit_cell_to_row(&mut self) {
        self.selection.exit_to_row();
        self.cell_edit.clear();
        self.pending_write_preview = None;
    }

    // Caller must set `input_mode` to `Normal` if it was `CellEdit` (SAB-136).
    pub fn exit_row_to_scroll(&mut self) {
        self.selection.reset();
        self.cell_edit.clear();
        self.staged_delete_rows.clear();
        self.pending_write_preview = None;
    }

    pub fn clear_operator_pending(&mut self, keep_delete: bool, keep_yank: bool) {
        if !keep_delete {
            self.delete_op_pending = false;
        }
        if !keep_yank {
            self.yank_op_pending = false;
        }
    }

    pub fn clear_expired_flash(&mut self, now: Instant) {
        if let Some(flash) = self.yank_flash
            && now >= flash.until
        {
            self.yank_flash = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::shared::ui_state::ResultNavMode;
    use std::time::Duration;

    #[test]
    fn reset_view_clears_scroll_and_all_interaction() {
        let mut ri = ResultInteraction {
            scroll_offset: 10,
            horizontal_offset: 5,
            ..Default::default()
        };
        ri.enter_row(3);
        ri.enter_cell(2);
        ri.begin_cell_edit(3, 2, "val".to_string());
        ri.stage_row(1);
        ri.set_write_preview(test_preview());

        ri.reset_view();

        assert_eq!(ri.scroll_offset, 0);
        assert_eq!(ri.horizontal_offset, 0);
        assert_eq!(ri.selection().mode(), ResultNavMode::Scroll);
        assert!(!ri.cell_edit().is_active());
        assert!(ri.staged_delete_rows().is_empty());
        assert!(ri.pending_write_preview().is_none());
    }

    #[test]
    fn reset_interaction_preserves_scroll() {
        let mut ri = ResultInteraction {
            scroll_offset: 10,
            horizontal_offset: 5,
            ..Default::default()
        };
        ri.enter_row(3);
        ri.stage_row(1);

        ri.reset_interaction();

        assert_eq!(ri.scroll_offset, 10);
        assert_eq!(ri.horizontal_offset, 5);
        assert_eq!(ri.selection().mode(), ResultNavMode::Scroll);
        assert!(ri.staged_delete_rows().is_empty());
    }

    #[test]
    fn discard_cell_edit_clears_edit_and_preview_only() {
        let mut ri = ResultInteraction::default();
        ri.enter_row(2);
        ri.enter_cell(4);
        ri.begin_cell_edit(2, 4, "val".to_string());
        ri.set_write_preview(test_preview());
        ri.stage_row(0);

        ri.discard_cell_edit();

        assert!(!ri.cell_edit().is_active());
        assert!(ri.pending_write_preview().is_none());
        assert_eq!(ri.selection().mode(), ResultNavMode::CellActive);
        assert!(ri.staged_delete_rows().contains(&0));
    }

    #[test]
    fn exit_cell_to_row_transitions_to_row_active() {
        let mut ri = ResultInteraction::default();
        ri.enter_row(2);
        ri.enter_cell(4);
        ri.begin_cell_edit(2, 4, "val".to_string());
        ri.set_write_preview(test_preview());
        ri.stage_row(0);

        ri.exit_cell_to_row();

        assert_eq!(ri.selection().mode(), ResultNavMode::RowActive);
        assert_eq!(ri.selection().row(), Some(2));
        assert!(!ri.cell_edit().is_active());
        assert!(ri.pending_write_preview().is_none());
        assert!(ri.staged_delete_rows().contains(&0));
    }

    #[test]
    fn exit_row_to_scroll_clears_selection_and_staging() {
        let mut ri = ResultInteraction::default();
        ri.enter_row(2);
        ri.begin_cell_edit(2, 0, "val".to_string());
        ri.stage_row(0);
        ri.stage_row(1);
        ri.set_write_preview(test_preview());

        ri.exit_row_to_scroll();

        assert_eq!(ri.selection().mode(), ResultNavMode::Scroll);
        assert!(!ri.cell_edit().is_active());
        assert!(ri.staged_delete_rows().is_empty());
        assert!(ri.pending_write_preview().is_none());
    }

    #[test]
    fn clear_expired_flash_removes_expired() {
        let now = Instant::now();
        let mut ri = ResultInteraction {
            yank_flash: Some(YankFlash {
                row: 0,
                col: None,
                until: now.checked_sub(Duration::from_millis(1)).unwrap(),
            }),
            ..Default::default()
        };

        ri.clear_expired_flash(now);

        assert!(ri.yank_flash.is_none());
    }

    #[test]
    fn clear_expired_flash_keeps_active() {
        let now = Instant::now();
        let mut ri = ResultInteraction {
            yank_flash: Some(YankFlash {
                row: 0,
                col: None,
                until: now + Duration::from_secs(1),
            }),
            ..Default::default()
        };

        ri.clear_expired_flash(now);

        assert!(ri.yank_flash.is_some());
    }

    #[test]
    fn clear_operator_pending_selective() {
        let mut ri = ResultInteraction {
            delete_op_pending: true,
            yank_op_pending: true,
            ..Default::default()
        };

        ri.clear_operator_pending(true, false);

        assert!(ri.delete_op_pending);
        assert!(!ri.yank_op_pending);
    }

    #[test]
    fn enter_row_delegates_correctly() {
        let mut ri = ResultInteraction::default();

        ri.enter_row(5);

        assert_eq!(ri.selection().mode(), ResultNavMode::RowActive);
        assert_eq!(ri.selection().row(), Some(5));
    }

    #[test]
    fn stage_row_and_unstage_last() {
        let mut ri = ResultInteraction::default();
        ri.stage_row(0);
        ri.stage_row(3);

        ri.unstage_last_row();

        assert_eq!(ri.staged_delete_rows().len(), 1);
        assert!(ri.staged_delete_rows().contains(&0));
    }

    #[test]
    fn begin_cell_edit_sets_active() {
        let mut ri = ResultInteraction::default();

        ri.begin_cell_edit(1, 2, "hello".to_string());

        assert!(ri.cell_edit().is_active());
        assert_eq!(ri.cell_edit().row, Some(1));
        assert_eq!(ri.cell_edit().col, Some(2));
    }

    #[test]
    fn clamp_selection_delegates() {
        let mut ri = ResultInteraction::default();
        ri.enter_row(10);

        ri.clamp_selection(5, 5);

        assert_eq!(ri.selection().mode(), ResultNavMode::Scroll);
    }

    fn test_preview() -> WritePreview {
        use crate::app::policy::write::write_guardrails::*;
        WritePreview {
            operation: WriteOperation::Update,
            sql: "UPDATE t SET x=1".to_string(),
            target_summary: TargetSummary {
                schema: "public".to_string(),
                table: "t".to_string(),
                key_values: vec![],
            },
            diff: vec![],
            guardrail: GuardrailDecision {
                risk_level: RiskLevel::Low,
                blocked: false,
                reason: None,
                target_summary: None,
            },
        }
    }
}
