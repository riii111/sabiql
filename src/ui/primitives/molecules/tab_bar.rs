use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use crate::theme::ThemePalette;

pub(crate) fn tab_span(
    label: impl Into<String>,
    selected: bool,
    theme: &ThemePalette,
) -> Span<'static> {
    let style = if selected {
        Style::default()
            .fg(theme.component.navigation.tab_active)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(theme.component.navigation.tab_inactive)
    };
    Span::styled(label.into(), style)
}
