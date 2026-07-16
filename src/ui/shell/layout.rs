use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::render_output::{
    BrowseLayout, ConfirmPreviewLayout, DetailLayout, ExplorerLayout, InputLayout, InspectorLayout,
    OverlayLayout, PickerLayouts, ResultLayout,
};
use crate::app::model::shared::ui_state::explorer_content_width_from_pane_width;
use crate::app::ports::outbound::RenderOutput;
use crate::app::services::AppServices;
use crate::features::browse::explorer::Explorer;
use crate::features::browse::inspector::Inspector;
use crate::features::browse::jsonb_detail::JsonbDetail;
use crate::features::browse::result::ResultPane;
use crate::features::browse::row_detail::RowDetail;
use crate::features::connections::error::ConnectionError;
use crate::features::connections::selector::ConnectionSelector;
use crate::features::connections::setup::ConnectionSetup;
use crate::features::overlays::confirm_dialog::ConfirmDialog;
use crate::features::overlays::help::HelpOverlay;
use crate::features::overlays::settings::SettingsOverlay;
use crate::features::pickers::command_palette::CommandPalette;
use crate::features::pickers::er_table_picker::ErTablePicker;
use crate::features::pickers::query_history_picker::QueryHistoryPicker;
use crate::features::pickers::table_picker::TablePicker;
use crate::features::sql_modal::SqlModal;
use crate::shell::command_line::CommandLine;
use crate::shell::footer::Footer;
use crate::shell::header::Header;
use crate::theme::{ThemePalette, palette_for};

pub struct MainLayout;

impl MainLayout {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        time_ms: Option<u128>,
        services: &AppServices,
        now: Instant,
    ) -> RenderOutput {
        Self::render_impl(
            frame,
            state,
            time_ms,
            services,
            now,
            palette_for(state.ui.theme_id()),
        )
    }

    // `render_with_theme` exists only as a test seam for injected palettes.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn render_with_theme(
        frame: &mut Frame,
        state: &AppState,
        time_ms: Option<u128>,
        services: &AppServices,
        now: Instant,
        theme: &ThemePalette,
    ) -> RenderOutput {
        Self::render_impl(frame, state, time_ms, services, now, theme)
    }

    fn render_impl(
        frame: &mut Frame,
        state: &AppState,
        time_ms: Option<u128>,
        services: &AppServices,
        now: Instant,
        theme: &ThemePalette,
    ) -> RenderOutput {
        let area = frame.area();

        let [header_area, main_area, footer_area, cmdline_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        Header::render(frame, header_area, state, theme);
        let browse = Self::render_browse_mode(frame, main_area, state, services, now, theme);

        Footer::render(frame, footer_area, state, services, time_ms, theme);
        let command_line_visible_width = CommandLine::render(frame, cmdline_area, state, theme);
        let connection_list_pane_height = match state.input_mode() {
            InputMode::ConnectionSelector => Some(ConnectionSelector::render(frame, state, theme)),
            _ => None,
        };

        let table_picker = match state.input_mode() {
            InputMode::TablePicker => Some(TablePicker::render(frame, state, theme)),
            _ => None,
        };

        let er_picker = match state.input_mode() {
            InputMode::ErTablePicker => Some(ErTablePicker::render(frame, state, theme)),
            _ => None,
        };

        let query_history_picker = match state.input_mode() {
            InputMode::QueryHistoryPicker => Some(QueryHistoryPicker::render(frame, state, theme)),
            _ => None,
        };

        let confirm_preview = match state.input_mode() {
            InputMode::ConfirmDialog => ConfirmDialog::render(frame, state, theme),
            _ => ConfirmPreviewLayout::default(),
        };

        let explain_compare_viewport_height = if matches!(state.input_mode(), InputMode::SqlModal) {
            SqlModal::render(frame, state, services, now, theme)
        } else {
            None
        };

        let jsonb_detail = match state.input_mode() {
            InputMode::JsonbDetail | InputMode::JsonbEdit => {
                JsonbDetail::render(frame, state, now, theme)
            }
            _ => None,
        };

        let row_detail = match state.input_mode() {
            InputMode::RowDetail => RowDetail::render(frame, state, now, theme),
            _ => None,
        };

        match state.input_mode() {
            InputMode::CommandPalette => CommandPalette::render(frame, state, theme),
            InputMode::Settings => SettingsOverlay::render(frame, state, theme),
            InputMode::Help => HelpOverlay::render(frame, state, theme),
            InputMode::ConnectionSetup => ConnectionSetup::render(frame, state, services, theme),
            InputMode::ConnectionError => ConnectionError::render(frame, state, now, theme),
            _ => {}
        }

        RenderOutput {
            browse,
            input: InputLayout {
                command_line_visible_width: Some(command_line_visible_width),
            },
            pickers: PickerLayouts {
                connection_list_pane_height,
                table: table_picker,
                er: er_picker,
                query_history: query_history_picker,
            },
            details: DetailLayout {
                jsonb: jsonb_detail,
                row: row_detail,
            },
            overlays: OverlayLayout {
                confirm_preview,
                explain_compare_viewport_height,
            },
        }
    }

    fn render_browse_mode(
        frame: &mut Frame,
        main_area: Rect,
        state: &AppState,
        services: &AppServices,
        now: Instant,
        theme: &ThemePalette,
    ) -> BrowseLayout {
        if state.ui.is_focus_mode() {
            let (result_plan, result_widths_cache) =
                ResultPane::render(frame, main_area, state, now, theme);
            BrowseLayout {
                explorer: ExplorerLayout::default(),
                inspector: InspectorLayout::default(),
                result: ResultLayout {
                    viewport_plan: result_plan,
                    widths_cache: result_widths_cache,
                    pane_height: main_area.height,
                },
            }
        } else {
            let [left_area, right_area] =
                Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)])
                    .areas(main_area);

            Explorer::render(frame, left_area, state, theme);

            let [inspector_area, result_area] =
                Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(right_area);

            let inspector_plan =
                Inspector::render(frame, inspector_area, state, services, now, theme);
            let (result_plan, result_widths_cache) =
                ResultPane::render(frame, result_area, state, now, theme);

            BrowseLayout {
                explorer: ExplorerLayout {
                    pane_height: left_area.height,
                    content_width: explorer_content_width_from_pane_width(left_area.width),
                },
                inspector: InspectorLayout {
                    viewport_plan: inspector_plan,
                    pane_height: inspector_area.height,
                },
                result: ResultLayout {
                    viewport_plan: result_plan,
                    widths_cache: result_widths_cache,
                    pane_height: result_area.height,
                },
            }
        }
    }
}
