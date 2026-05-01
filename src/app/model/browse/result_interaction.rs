use std::collections::BTreeSet;
use std::time::Instant;

use super::cell_edit::CellEditState;
use crate::model::shared::text_input::TextInputState;
use crate::model::shared::ui_state::{ResultSelection, YankFlash};
use crate::policy::write::write_guardrails::WritePreview;

// Invariants:
// - `reset_view` / `reset_interaction` clear staged deletes too.
// - `exit_cell_to_scroll()` preserves staged deletes so Esc does not drop a staged batch.
// - Callers must restore `input_mode` themselves when leaving `CellEdit`.
#[derive(Debug, Clone, Default)]
pub struct ResultInteraction {
    pub scroll_offset: usize,
    pub horizontal_offset: usize,
    pub yank_flash: Option<YankFlash>,

    delete_op_pending: bool,
    yank_op_pending: bool,
    selection: ResultSelection,
    cell_edit: CellEditState,
    staged_delete_rows: BTreeSet<usize>,
    pending_write_preview: Option<WritePreview>,
}

impl ResultInteraction {
    fn clear_active_cell_state(&mut self) {
        self.selection.reset();
        self.cell_edit.clear();
        self.pending_write_preview = None;
    }

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

    pub fn activate_cell(&mut self, row: usize, col: usize) {
        self.selection.enter_cell(row, col);
    }

    pub fn move_row(&mut self, row: usize) {
        self.selection.move_row(row);
    }

    pub fn move_cell(&mut self, col: usize) {
        self.selection.move_cell(col);
    }

    pub fn clamp_selection(&mut self, max_rows: usize, max_cols: usize) {
        self.selection.clamp(max_rows, max_cols);
    }

    pub fn begin_cell_edit(&mut self, row: usize, col: usize, value: String) {
        self.cell_edit.begin(row, col, value);
    }

    pub fn cell_edit_input_mut(&mut self) -> &mut TextInputState {
        self.cell_edit.input_mut()
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
        self.reset_interaction();
    }

    pub fn reset_interaction(&mut self) {
        self.clear_active_cell_state();
        self.staged_delete_rows.clear();
    }

    // Caller must set `input_mode` to `Normal` if it was `CellEdit` (SAB-136).
    pub fn exit_cell_to_scroll(&mut self) {
        self.clear_active_cell_state();
    }

    pub fn start_delete_operator(&mut self) {
        self.delete_op_pending = true;
        self.yank_op_pending = false;
    }

    pub fn start_yank_operator(&mut self) {
        self.yank_op_pending = true;
        self.delete_op_pending = false;
    }

    pub fn clear_operator_pending(&mut self) {
        self.delete_op_pending = false;
        self.yank_op_pending = false;
    }

    pub fn is_delete_operator_pending(&self) -> bool {
        self.delete_op_pending
    }

    pub fn is_yank_operator_pending(&self) -> bool {
        self.yank_op_pending
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
    use crate::model::shared::ui_state::ResultNavMode;
    use std::time::Duration;

    #[test]
    fn reset_view_clears_scroll_and_all_interaction() {
        let mut ri = ResultInteraction {
            scroll_offset: 10,
            horizontal_offset: 5,
            ..Default::default()
        };
        ri.activate_cell(3, 2);
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
        ri.activate_cell(3, 0);
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
        ri.activate_cell(2, 4);
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
    fn exit_cell_to_scroll_clears_selection_and_preserves_staging() {
        let mut ri = ResultInteraction::default();
        ri.activate_cell(2, 4);
        ri.begin_cell_edit(2, 4, "val".to_string());
        ri.set_write_preview(test_preview());
        ri.stage_row(0);

        ri.exit_cell_to_scroll();

        assert_eq!(ri.selection().mode(), ResultNavMode::Scroll);
        assert!(!ri.cell_edit().is_active());
        assert!(ri.pending_write_preview().is_none());
        assert!(ri.staged_delete_rows().contains(&0));
    }

    #[test]
    fn exit_cell_to_scroll_clears_pending_preview_even_without_staging() {
        let mut ri = ResultInteraction::default();
        ri.activate_cell(2, 0);
        ri.begin_cell_edit(2, 0, "val".to_string());
        ri.set_write_preview(test_preview());

        ri.exit_cell_to_scroll();

        assert_eq!(ri.selection().mode(), ResultNavMode::Scroll);
        assert!(!ri.cell_edit().is_active());
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

    mod operator_pending {
        use super::*;

        #[test]
        fn clear_clears_both() {
            let mut ri = ResultInteraction::default();
            ri.start_delete_operator();

            ri.clear_operator_pending();

            assert!(!ri.is_delete_operator_pending());
            assert!(!ri.is_yank_operator_pending());

            ri.start_yank_operator();

            ri.clear_operator_pending();

            assert!(!ri.is_delete_operator_pending());
            assert!(!ri.is_yank_operator_pending());
        }

        #[test]
        fn start_yank_clears_delete() {
            let mut ri = ResultInteraction::default();

            ri.start_delete_operator();
            ri.start_yank_operator();

            assert!(!ri.is_delete_operator_pending());
            assert!(ri.is_yank_operator_pending());
        }

        #[test]
        fn start_delete_clears_yank() {
            let mut ri = ResultInteraction::default();

            ri.start_yank_operator();
            ri.start_delete_operator();

            assert!(ri.is_delete_operator_pending());
            assert!(!ri.is_yank_operator_pending());
        }
    }

    #[test]
    fn enter_cell_delegates_correctly() {
        let mut ri = ResultInteraction::default();

        ri.activate_cell(5, 2);

        assert_eq!(ri.selection().mode(), ResultNavMode::CellActive);
        assert_eq!(ri.selection().row(), Some(5));
        assert_eq!(ri.selection().cell(), Some(2));
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
        assert_eq!(ri.cell_edit().row(), Some(1));
        assert_eq!(ri.cell_edit().col(), Some(2));
    }

    #[test]
    fn clamp_selection_delegates() {
        let mut ri = ResultInteraction::default();
        ri.activate_cell(10, 0);

        ri.clamp_selection(5, 5);

        assert_eq!(ri.selection().mode(), ResultNavMode::Scroll);
    }

    fn test_preview() -> WritePreview {
        use crate::policy::write::write_guardrails::*;
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
