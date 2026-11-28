use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::input_mode::InputMode;
use crate::app::model::shared::render_output::{
    BrowseLayout, ConfirmPreviewLayout, DetailLayout, ExplorerLayout, InputLayout, InspectorLayout,
    OverlayLayout, PaneArea, PaneAreas, PickerLayouts, ResultLayout,
};
use crate::app::ports::outbound::{CellDetailViewport, RenderOutput};
use crate::app::services::AppServices;
use crate::features::browse::cell_detail::{CellDetail, CellDetailRenderMetrics};
use crate::features::browse::explorer::Explorer;
use crate::features::browse::inspector::Inspector;
use crate::features::browse::json_detail::JsonDetail;
use crate::features::browse::result::ResultPane;
use crate::features::browse::row_detail::RowDetail;
use crate::features::connections::error::ConnectionError;
use crate::features::connections::selector::ConnectionSelector;
use crate::features::connections::setup::ConnectionSetup;
use crate::features::overlays::confirm_dialog::ConfirmDialog;
use crate::features::overlays::help::HelpOverlay;
use crate::features::overlays::settings::SettingsOverlay;
use crate::features::overlays::sqlite_diagnostics::SqliteDiagnosticsOverlay;
use crate::features::pickers::command_palette::CommandPalette;
use crate::features::pickers::er_table_picker::ErTablePicker;
use crate::features::pickers::query_history_picker::QueryHistoryPicker;
use crate::features::pickers::table_picker::TablePicker;
use crate::features::sql_modal::SqlModal;
use crate::shell::command_line::CommandLine;
use crate::shell::footer::Footer;
use crate::shell::header::Header;
use crate::shell::motion_fx::{
    ModalAfterimage, apply_focus_transition, apply_modal_fade, apply_modal_fade_out,
};
use crate::theme::{ThemePalette, palette_for};

pub struct MainLayout;

impl MainLayout {
    pub fn render(
        frame: &mut Frame,
        state: &AppState,
        time_ms: Option<u128>,
        services: &AppServices,
        now: Instant,
        afterimage: &mut ModalAfterimage,
    ) -> RenderOutput {
        Self::render_impl(
            frame,
            state,
            time_ms,
            services,
            now,
            palette_for(state.ui.theme_id()),
            Some(afterimage),
        )
    }

    fn render_impl(
        frame: &mut Frame,
        state: &AppState,
        time_ms: Option<u128>,
        services: &AppServices,
        now: Instant,
        theme: &ThemePalette,
        afterimage: Option<&mut ModalAfterimage>,
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
        if let Some(transition) = state.motion.focus_transition(now) {
            apply_focus_transition(frame.buffer_mut(), &browse.pane_areas, transition, theme);
        }

        Footer::render(frame, footer_area, state, time_ms, theme);
        let command_line_visible_width = CommandLine::render(frame, cmdline_area, state, theme);

        let shows_overlay = state.input_mode().shows_overlay();
        let modal_fade = shows_overlay
            .then(|| state.motion.modal_progress(now))
            .flatten();
        // The modal fades in against the panes drawn before it, and its image is kept against them for fading out.
        let background = (shows_overlay && (modal_fade.is_some() || afterimage.is_some()))
            .then(|| frame.buffer_mut().clone());
        let connection_list_pane_height = match state.input_mode() {
            InputMode::ConnectionSelector => Some(ConnectionSelector::render(frame, state, theme)),
            _ => None,
        };

        let table_picker = match state.input_mode() {
            InputMode::TablePicker => Some(TablePicker::render(frame, state, theme)),
            // The palette shares the table picker's selection and filter state, so it reports into the same layout slot.
            InputMode::CommandPalette => Some(CommandPalette::render(frame, state, theme)),
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
            SqlModal::render(frame, state, now, theme)
        } else {
            None
        };

        let json_detail = match state.input_mode() {
            InputMode::JsonDetail | InputMode::JsonEdit => {
                JsonDetail::render(frame, state, now, theme)
            }
            _ => None,
        };

        let cell_detail = match state.input_mode() {
            InputMode::CellDetail => CellDetail::render(frame, state, now, theme).map(
                |CellDetailRenderMetrics {
                     visible_rows,
                     viewport_width,
                 }| CellDetailViewport {
                    visible_rows,
                    viewport_width,
                },
            ),
            _ => None,
        };

        let row_detail = match state.input_mode() {
            InputMode::RowDetail => RowDetail::render(frame, state, now, theme),
            _ => None,
        };

        let (sqlite_diagnostics_content_line_count, sqlite_diagnostics_viewport_height) =
            match state.input_mode() {
                InputMode::SqliteDiagnostics => {
                    let metrics = SqliteDiagnosticsOverlay::render(frame, state, now, theme);
                    (
                        Some(metrics.content_line_count),
                        Some(metrics.viewport_height),
                    )
                }
                _ => (None, None),
            };

        match state.input_mode() {
            InputMode::Settings => SettingsOverlay::render(frame, state, theme),
            InputMode::Help => HelpOverlay::render(frame, state, now, theme),
            InputMode::ConnectionSetup => ConnectionSetup::render(frame, state, services, theme),
            InputMode::ConnectionError => ConnectionError::render(frame, state, now, theme),
            _ => {}
        }

        if let Some(afterimage) = afterimage {
            match (&background, state.motion.modal_close_visibility(now)) {
                (Some(background), _) => afterimage.capture(frame.buffer_mut(), background, theme),
                (None, Some(visibility)) => {
                    apply_modal_fade_out(frame.buffer_mut(), afterimage, visibility, theme);
                }
                (None, None) => afterimage.clear(),
            }
        }
        if let (Some(progress), Some(background)) = (modal_fade, &background) {
            apply_modal_fade(frame.buffer_mut(), background, progress, theme);
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
                json: json_detail,
                cell: cell_detail,
                row: row_detail,
            },
            overlays: OverlayLayout {
                confirm_preview,
                explain_compare_viewport_height,
                sqlite_diagnostics_content_line_count,
                sqlite_diagnostics_viewport_height,
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
        let collapse = state
            .motion
            .focus_mode_collapse(now)
            .unwrap_or_else(|| if state.ui.is_focus_mode() { 1.0 } else { 0.0 });
        if collapse >= 1.0 {
            let (result_plan, result_widths_cache) =
                ResultPane::render(frame, main_area, state, now, theme);
            return BrowseLayout {
                explorer: ExplorerLayout::default(),
                inspector: InspectorLayout::default(),
                result: ResultLayout {
                    viewport_plan: result_plan,
                    widths_cache: result_widths_cache,
                    pane_height: main_area.height,
                },
                pane_areas: PaneAreas {
                    explorer: None,
                    inspector: None,
                    result: Some(pane_area(main_area)),
                },
            };
        }

        let [left_area, right_area] =
            Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)])
                .areas(main_area);
        let [inspector_area, _] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(right_area);
        let explorer_width = collapsed_length(left_area.width, collapse);
        let inspector_height = collapsed_length(inspector_area.height, collapse);

        let (left_area, right_area) = split_columns(main_area, explorer_width);
        let (inspector_area, result_area) = split_rows(right_area, inspector_height);

        let explorer = left_area.map(|area| ExplorerLayout {
            pane_height: area.height,
            content_width: Explorer::render(frame, area, state, now, theme),
        });
        let inspector = inspector_area.map(|area| InspectorLayout {
            viewport_plan: Inspector::render(frame, area, state, services, now, theme),
            pane_height: area.height,
        });
        let (result_plan, result_widths_cache) =
            ResultPane::render(frame, result_area, state, now, theme);

        BrowseLayout {
            explorer: explorer.unwrap_or_default(),
            inspector: inspector.unwrap_or_default(),
            result: ResultLayout {
                viewport_plan: result_plan,
                widths_cache: result_widths_cache,
                pane_height: result_area.height,
            },
            pane_areas: PaneAreas {
                explorer: left_area.map(pane_area),
                inspector: inspector_area.map(pane_area),
                result: Some(pane_area(result_area)),
            },
        }
    }
}

// A pane narrower than this cannot draw its border and one cell of content, so it is dropped.
const MIN_PANE_EXTENT: u16 = 3;

fn collapsed_length(full: u16, collapse: f32) -> u16 {
    (f32::from(full) * (1.0 - collapse)).round() as u16
}

/// A column narrower than `MIN_PANE_EXTENT` is left to the remaining area.
fn split_columns(area: Rect, width: u16) -> (Option<Rect>, Rect) {
    if width < MIN_PANE_EXTENT {
        return (None, area);
    }
    let [left, right] =
        Layout::horizontal([Constraint::Length(width), Constraint::Fill(1)]).areas(area);
    (Some(left), right)
}

/// A row shorter than `MIN_PANE_EXTENT` is left to the remaining area.
fn split_rows(area: Rect, height: u16) -> (Option<Rect>, Rect) {
    if height < MIN_PANE_EXTENT {
        return (None, area);
    }
    let [top, bottom] =
        Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
    (Some(top), bottom)
}

fn pane_area(rect: Rect) -> PaneArea {
    PaneArea {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::{AppServices, AppState, Frame, Instant, MainLayout, RenderOutput, ThemePalette};

    impl MainLayout {
        #[doc(hidden)]
        pub fn render_with_theme(
            frame: &mut Frame,
            state: &AppState,
            time_ms: Option<u128>,
            services: &AppServices,
            now: Instant,
            theme: &ThemePalette,
        ) -> RenderOutput {
            Self::render_impl(frame, state, time_ms, services, now, theme, None)
        }
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

    const AREA: Rect = Rect::new(0, 0, 100, 40);

    #[test]
    fn uncollapsed_length_is_unchanged() {
        assert_eq!(collapsed_length(25, 0.0), 25);
        assert_eq!(collapsed_length(25, 1.0), 0);
    }

    #[test]
    fn column_split_gives_the_rest_to_the_right() {
        let (left, right) = split_columns(AREA, 25);

        assert_eq!(left, Some(Rect::new(0, 0, 25, 40)));
        assert_eq!(right, Rect::new(25, 0, 75, 40));
    }

    #[test]
    fn too_narrow_column_is_dropped() {
        let (left, right) = split_columns(AREA, MIN_PANE_EXTENT - 1);

        assert_eq!(left, None);
        assert_eq!(right, AREA);
    }

    #[test]
    fn too_short_row_is_dropped() {
        let (top, bottom) = split_rows(AREA, MIN_PANE_EXTENT - 1);

        assert_eq!(top, None);
        assert_eq!(bottom, AREA);
    }
}
