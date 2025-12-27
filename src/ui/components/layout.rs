use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use super::command_palette::CommandPalette;
use super::explorer::Explorer;
use super::footer::Footer;
use super::header::Header;
use super::help_overlay::HelpOverlay;
use super::inspector::Inspector;
use super::result::ResultPane;
use super::table_picker::TablePicker;
use super::tabs::Tabs;
use crate::app::state::AppState;

pub struct MainLayout;

impl MainLayout {
    pub fn render(frame: &mut Frame, state: &AppState) {
        let area = frame.area();

        let [header_area, tabs_area, main_area, footer_area, _cmdline_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        Header::render(frame, header_area, state);
        Tabs::render(frame, tabs_area, state);

        let [left_area, right_area] =
            Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
                .areas(main_area);

        Explorer::render(frame, left_area, state);

        let [inspector_area, result_area] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(right_area);

        Inspector::render(frame, inspector_area, state);
        ResultPane::render(frame, result_area, state);

        Footer::render(frame, footer_area, state);

        // Render overlays (on top of everything else)
        if state.show_table_picker {
            TablePicker::render(frame, state);
        }
        if state.show_command_palette {
            CommandPalette::render(frame, state);
        }
        if state.show_help {
            HelpOverlay::render(frame, state);
        }
    }
}
