use super::{PickerModal, render_picker, render_picker_list};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::ListItem;

use crate::app::model::app_state::AppState;
use crate::app::model::shared::render_output::PickerLayout;
use crate::app::update::action::{Action, ModalKind};
use crate::app::update::input::keymap::listed_palette_entries;
use crate::primitives::atoms::Icon;
use crate::theme::ThemePalette;

pub(crate) struct CommandPalette;

impl CommandPalette {
    pub(crate) fn render(
        frame: &mut Frame,
        state: &AppState,
        theme: &ThemePalette,
    ) -> PickerLayout {
        let picker = state.ui.table_picker();
        let (list_area, visible_width) =
            render_picker(frame, state, theme, PickerModal::CommandPalette, |inner| {
                let areas: [_; 2] =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
                areas.into()
            });

        let items: Vec<ListItem> = listed_palette_entries(state)
            .iter()
            .map(|kb| {
                let style = Style::default().fg(theme.semantic.text.secondary);
                let icon = command_icon(&kb.action)
                    .map_or("", |icon| icon.prefix(state.settings.saved_icon_mode()));
                let content = format!("  {:<18} {icon}{}", kb.key, kb.description);
                ListItem::new(content).style(style)
            })
            .collect();

        render_picker_list(
            frame,
            list_area,
            items,
            Some(picker.selected()),
            picker.scroll_offset(),
            theme,
        );

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
