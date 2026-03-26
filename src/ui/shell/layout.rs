use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::viewport::ViewportPlan;
use crate::app::ports::RenderOutput;
use crate::app::services::AppServices;
use crate::ui::features::browse::explorer::Explorer;
use crate::ui::features::browse::inspector::Inspector;
use crate::ui::features::browse::result::ResultPane;
use crate::ui::features::connections::error::ConnectionError;
use crate::ui::features::connections::selector::ConnectionSelector;
use crate::ui::features::connections::setup::ConnectionSetup;
use crate::ui::features::overlays::confirm_dialog::ConfirmDialog;
use crate::ui::features::overlays::help::HelpOverlay;
use crate::ui::features::pickers::command_palette::CommandPalette;
use crate::ui::features::pickers::er_table_picker::ErTablePicker;
use crate::ui::features::pickers::query_history_picker::QueryHistoryPicker;
use crate::ui::features::pickers::table_picker::TablePicker;
use crate::ui::features::sql_modal::SqlModal;
use crate::ui::shell::command_line::CommandLine;
use crate::ui::shell::footer::Footer;
use crate::ui::shell::header::Header;

pub struct MainLayout;

impl MainLayout {
    pub fn render(
        frame: &mut Frame,
        state: &mut AppState,
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
        CommandLine::render(frame, cmdline_area, state);

        match state.input_mode() {
            InputMode::TablePicker => TablePicker::render(frame, state),
            InputMode::ErTablePicker => ErTablePicker::render(frame, state),
            InputMode::QueryHistoryPicker => QueryHistoryPicker::render(frame, state),
            InputMode::CommandPalette => CommandPalette::render(frame, state),
            InputMode::Help => HelpOverlay::render(frame, state),
            InputMode::SqlModal => SqlModal::render(frame, state, now),
            InputMode::ConnectionSetup => ConnectionSetup::render(frame, state),
            InputMode::ConnectionError => ConnectionError::render(frame, state, now),
            InputMode::ConfirmDialog => ConfirmDialog::render(frame, state),
            InputMode::ConnectionSelector => ConnectionSelector::render(frame, state),
            _ => {}
        }

        output
    }

    fn render_browse_mode(
        frame: &mut Frame,
        main_area: Rect,
        state: &mut AppState,
        services: &AppServices,
        now: Instant,
    ) -> RenderOutput {
        if state.ui.focus_mode {
            let (result_plan, result_widths_cache) =
                ResultPane::render(frame, main_area, state, now);
            RenderOutput {
                inspector_viewport_plan: ViewportPlan::default(),
                result_viewport_plan: result_plan,
                result_widths_cache,
                explorer_pane_height: 0,
                inspector_pane_height: 0,
                result_pane_height: main_area.height,
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
                inspector_pane_height: inspector_area.height,
                result_pane_height: result_area.height,
            }
        }
    }
}
