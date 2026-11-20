use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::style::Style;
use ratatui::widgets::{List, ListItem};

use crate::app::model::app_state::AppState;
use crate::app::update::action::{Action, ModalKind};
use crate::app::update::input::keymap::{HintSurface, Picker, picker_hints};
use crate::app::update::input::palette::palette_commands;
use crate::primitives::atoms::Icon;
use crate::theme::ThemePalette;

use crate::primitives::molecules::{FooterHintBar, render_modal};

pub struct CommandPalette;

impl CommandPalette {
    pub fn render(frame: &mut Frame, state: &AppState, theme: &ThemePalette) {
        let (_, inner) = render_modal(
            frame,
            Constraint::Percentage(50),
            Constraint::Percentage(50),
            " Command Palette ",
            FooterHintBar::new(
                picker_hints(Picker::CommandPalette, HintSurface::Border)
                    .into_iter()
                    .map(|hint| (hint.key, hint.short)),
            ),
            theme,
        );

        let items: Vec<ListItem> = palette_commands(
            state.settings.saved_keymap_preset(),
            &state.session.active_engine_feature_profile(),
        )
        .enumerate()
        .map(|(i, kb)| {
            let style = if i == state.ui.table_picker().selected() {
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

        let list = List::new(items);
        frame.render_widget(list, inner);
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
