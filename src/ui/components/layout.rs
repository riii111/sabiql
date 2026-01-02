use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use super::command_line::CommandLine;
use super::command_palette::CommandPalette;
use super::explorer::Explorer;
use super::footer::Footer;
use super::header::Header;
use super::help_overlay::HelpOverlay;
use super::inspector::Inspector;
use super::result::ResultPane;
use super::sql_modal::SqlModal;
use super::table_picker::TablePicker;
use super::viewport_columns::ViewportPlan;
use crate::app::input_mode::InputMode;
use crate::app::state::AppState;

#[derive(Default)]
pub struct RenderOutput {
    pub inspector_viewport_plan: ViewportPlan,
    pub result_viewport_plan: ViewportPlan,
    pub inspector_pane_height: u16,
    pub result_pane_height: u16,
}

pub struct MainLayout;

impl MainLayout {
    pub fn render(frame: &mut Frame, state: &mut AppState, time_ms: Option<u128>) -> RenderOutput {
        let area = frame.area();

        let [header_area, main_area, footer_area, cmdline_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        Header::render(frame, header_area, state);
        let output = Self::render_browse_mode(frame, main_area, state);

        Footer::render(frame, footer_area, state, time_ms);
        CommandLine::render(frame, cmdline_area, state);

        match state.ui.input_mode {
            InputMode::TablePicker => TablePicker::render(frame, state),
            InputMode::CommandPalette => CommandPalette::render(frame, state),
            InputMode::Help => HelpOverlay::render(frame, state),
            InputMode::SqlModal => SqlModal::render(frame, state),
            _ => {}
        }

        output
    }

    fn render_browse_mode(
        frame: &mut Frame,
        main_area: Rect,
        state: &mut AppState,
    ) -> RenderOutput {
        if state.ui.focus_mode {
            let result_plan = ResultPane::render(frame, main_area, state);
            RenderOutput {
                inspector_viewport_plan: ViewportPlan::default(),
                result_viewport_plan: result_plan,
                inspector_pane_height: 0,
                result_pane_height: main_area.height,
            }
        } else {
            let [left_area, right_area] =
                Layout::horizontal([Constraint::Percentage(20), Constraint::Percentage(80)])
                    .areas(main_area);

            Explorer::render(frame, left_area, state);

            let [inspector_area, result_area] =
                Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(right_area);

            let inspector_plan = Inspector::render(frame, inspector_area, state);
            let result_plan = ResultPane::render(frame, result_area, state);

            RenderOutput {
                inspector_viewport_plan: inspector_plan,
                result_viewport_plan: result_plan,
                inspector_pane_height: inspector_area.height,
                result_pane_height: result_area.height,
            }
        }
    }
}
