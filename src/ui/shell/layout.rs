use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::ui_state::explorer_content_width_from_pane_width;
use crate::app::model::shared::viewport::ViewportPlan;
use crate::app::ports::RenderOutput;
use crate::app::services::AppServices;
use crate::ui::features::browse::explorer::Explorer;
use crate::ui::features::browse::inspector::Inspector;
use crate::ui::features::browse::jsonb_detail::JsonbDetail;
use crate::ui::features::browse::result::ResultPane;
use crate::ui::features::connections::error::ConnectionError;
use crate::ui::features::connections::selector::ConnectionSelector;
use crate::ui::features::connections::setup::ConnectionSetup;
use crate::ui::features::overlays::confirm_dialog::{ConfirmDialog, ConfirmPreviewMetrics};
use crate::ui::features::overlays::help::HelpOverlay;
use crate::ui::features::pickers::command_palette::CommandPalette;
use crate::ui::features::pickers::er_table_picker::{ErTablePicker, ErTablePickerRenderMetrics};
use crate::ui::features::pickers::query_history_picker::QueryHistoryPicker;
use crate::ui::features::pickers::table_picker::{TablePicker, TablePickerRenderMetrics};
use crate::ui::features::sql_modal::SqlModal;
use crate::ui::shell::command_line::CommandLine;
use crate::ui::shell::footer::Footer;
use crate::ui::shell::header::Header;

pub struct MainLayout;

impl MainLayout {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        time_ms: Option<u128>,
        services: &AppServices,
        now: Instant,
    ) -> RenderOutput {
        let area = frame.area();

        let [header_area, main_area, footer_area, cmdline_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        Header::render(frame, header_area, state);
        let output = Self::render_browse_mode(frame, main_area, state, services, now);

        Footer::render(frame, footer_area, state, time_ms);
        let command_line_visible_width = CommandLine::render(frame, cmdline_area, state);
        let connection_list_pane_height = match state.input_mode() {
            InputMode::ConnectionSelector => Some(ConnectionSelector::render(frame, state)),
            _ => None,
        };

        let (table_picker_pane_height, table_picker_filter_visible_width) = match state.input_mode()
        {
            InputMode::TablePicker => {
                let TablePickerRenderMetrics {
                    pane_height,
                    filter_visible_width,
                } = TablePicker::render(frame, state);
                (Some(pane_height), Some(filter_visible_width))
            }
            _ => (None, None),
        };

        let (er_picker_pane_height, er_picker_filter_visible_width) = match state.input_mode() {
            InputMode::ErTablePicker => {
                let ErTablePickerRenderMetrics {
                    pane_height,
                    filter_visible_width,
                } = ErTablePicker::render(frame, state);
                (Some(pane_height), Some(filter_visible_width))
            }
            _ => (None, None),
        };

        let query_history_picker_pane_height = match state.input_mode() {
            InputMode::QueryHistoryPicker => Some(QueryHistoryPicker::render(frame, state)),
            _ => None,
        };

        let (
            confirm_preview_viewport_height,
            confirm_preview_content_height,
            confirm_preview_scroll,
        ) = match state.input_mode() {
            InputMode::ConfirmDialog => {
                let ConfirmPreviewMetrics {
                    viewport_height,
                    content_height,
                    scroll,
                } = ConfirmDialog::render(frame, state);
                (viewport_height, content_height, scroll)
            }
            _ => (None, None, 0),
        };

        let explain_compare_viewport_height = if matches!(state.input_mode(), InputMode::SqlModal) {
            SqlModal::render(frame, state, now)
        } else {
            None
        };

        let jsonb_detail_scroll_offset = match state.input_mode() {
            InputMode::JsonbDetail | InputMode::JsonbEdit => JsonbDetail::render(frame, state, now),
            _ => None,
        };

        match state.input_mode() {
            InputMode::CommandPalette => CommandPalette::render(frame, state),
            InputMode::Help => HelpOverlay::render(frame, state),
            InputMode::ConnectionSetup => ConnectionSetup::render(frame, state),
            InputMode::ConnectionError => ConnectionError::render(frame, state, now),
            _ => {}
        }

        RenderOutput {
            command_line_visible_width: Some(command_line_visible_width),
            connection_list_pane_height,
            table_picker_pane_height,
            table_picker_filter_visible_width,
            er_picker_pane_height,
            er_picker_filter_visible_width,
            query_history_picker_pane_height,
            jsonb_detail_scroll_offset,
            confirm_preview_viewport_height,
            confirm_preview_content_height,
            confirm_preview_scroll,
            explain_compare_viewport_height,
            ..output
        }
    }

    fn render_browse_mode(
        frame: &mut Frame,
        main_area: Rect,
        state: &AppState,
        services: &AppServices,
        now: Instant,
    ) -> RenderOutput {
        if state.ui.is_focus_mode() {
            let (result_plan, result_widths_cache) =
                ResultPane::render(frame, main_area, state, now);
            RenderOutput {
                inspector_viewport_plan: ViewportPlan::default(),
                result_viewport_plan: result_plan,
                result_widths_cache,
                explorer_pane_height: 0,
                explorer_content_width: 0,
                inspector_pane_height: 0,
                result_pane_height: main_area.height,
                ..RenderOutput::default()
            }
        } else {
            let [left_area, right_area] =
                Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)])
                    .areas(main_area);

            Explorer::render(frame, left_area, state);

            let [inspector_area, result_area] =
                Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(right_area);

            let inspector_plan = Inspector::render(frame, inspector_area, state, services, now);
            let (result_plan, result_widths_cache) =
                ResultPane::render(frame, result_area, state, now);

            RenderOutput {
                inspector_viewport_plan: inspector_plan,
                result_viewport_plan: result_plan,
                result_widths_cache,
                explorer_pane_height: left_area.height,
                explorer_content_width: explorer_content_width_from_pane_width(left_area.width),
                inspector_pane_height: inspector_area.height,
                result_pane_height: result_area.height,
                ..RenderOutput::default()
            }
        }
    }
}
