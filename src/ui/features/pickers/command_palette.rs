use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::{List, ListItem, ListState};

use crate::app::model::app_state::AppState;
use crate::app::model::shared::render_output::PickerLayout;
use crate::app::update::action::{Action, ModalKind};
use crate::app::update::input::keymap::{
    HintSurface, KeyContext, Picker, listed_palette_entries, picker_hints,
};
use crate::primitives::atoms::Icon;
use crate::theme::ThemePalette;

use crate::primitives::molecules::{FooterHintBar, render_filter_input_line, render_modal};

pub(crate) struct CommandPalette;

impl CommandPalette {
    pub(crate) fn render(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> PickerLayout {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(50),
            Constraint::Percentage(50),
            " Command Palette ",
            FooterHintBar::new(
                picker_hints(
                    Picker::CommandPalette,
                    HintSurface::Border,
                    &KeyContext::from_state(state),
                )
                .into_iter()
                .map(|hint| (hint.key, hint.short)),
            ),
            theme,
        );

        let [filter_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        let picker = state.ui.table_picker();
        let visible_width =
            render_filter_input_line(frame, filter_area, picker.filter_input(), None, theme);

        let items: Vec<ListItem> = listed_palette_entries(state)
            .iter()
            .enumerate()
            .map(|(i, kb)| {
                let style = if i == picker.selected() {
                    theme.picker_selected_style()
                } else {
                    Style::default().fg(theme.semantic.text.secondary)
                };
                let icon = command_icon(&kb.action)
                    .map_or("", |icon| icon.prefix(state.settings.saved_icon_mode()));
                let content = format!("  {:<18} {icon}{}", kb.key, kb.description);
                ListItem::new(content).style(style)
            })
            .collect();

        // The list widget scrolls to the selection, so a resize never leaves the command Enter runs off screen.
        let mut list_state = ListState::default()
            .with_offset(picker.scroll_offset())
            .with_selected(Some(picker.selected()));
        frame.render_stateful_widget(List::new(items), list_area, &mut list_state);

        PickerLayout {
            pane_height: list_area.height,
            filter_visible_width: visible_width,
        }
    }
}

fn command_icon(action: &Action) -> Option<Icon> {
    Some(match action {
        Action::Quit => Icon::Quit,
        Action::ToggleHelp => Icon::Help,
        Action::OpenModal(ModalKind::TablePicker) => Icon::Table,
        Action::OpenModal(ModalKind::Settings) => Icon::Settings,
        Action::ToggleFocus => Icon::Focus,
        Action::ReloadMetadata => Icon::Reload,
        Action::OpenModal(ModalKind::SqlModal) => Icon::Query,
        Action::OpenModal(ModalKind::ErTablePicker) => Icon::Diagram,
        Action::OpenModal(ModalKind::ConnectionSelector) => Icon::Connections,
        Action::OpenModal(ModalKind::SqliteDiagnostics) => Icon::Diagnostics,
        Action::RequestCsvExport => Icon::Export,
        Action::ToggleReadOnly => Icon::ReadOnly,
        Action::OpenModal(ModalKind::QueryHistoryPicker) => Icon::History,
        _ => return None,
    })
}
