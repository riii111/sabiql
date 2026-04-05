use ratatui::style::{Color, Modifier, Style};

use crate::app::model::shared::theme_id::ThemeId;
use crate::app::policy::write::write_guardrails::RiskLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Success,
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub modal_border: Color,
    pub modal_border_highlight: Color,
    pub modal_title: Color,
    pub modal_hint: Color,
    pub key_chip_bg: Color,
    pub key_chip_fg: Color,
    pub editor_current_line_bg: Color,
    pub completion_selected_bg: Color,
    pub input_value: Color,
    pub note_text: Color,
    pub focus_border: Color,
    pub unfocus_border: Color,
    pub highlight_border: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    pub text_accent: Color,
    pub status_success: Color,
    pub status_error: Color,
    pub status_warning: Color,
    pub status_medium_risk: Color,
    pub cursor_fg: Color,
    pub section_header: Color,
    pub scrollbar_active: Color,
    pub scrollbar_inactive: Color,
    pub result_row_active_bg: Color,
    pub result_cell_active_bg: Color,
    pub cell_edit_fg: Color,
    pub cell_draft_pending_fg: Color,
    pub staged_delete_bg: Color,
    pub staged_delete_fg: Color,
    pub yank_flash_bg: Color,
    pub yank_flash_fg: Color,
    pub sql_keyword: Color,
    pub sql_string: Color,
    pub sql_number: Color,
    pub sql_comment: Color,
    pub sql_operator: Color,
    pub sql_text: Color,
    pub json_key: Color,
    pub json_string: Color,
    pub json_number: Color,
    pub json_bool: Color,
    pub json_null: Color,
    pub json_bracket: Color,
    pub striped_row_bg: Color,
    pub selection_bg: Color,
    pub tab_active: Color,
    pub tab_inactive: Color,
    pub active_indicator: Color,
    pub inactive_indicator: Color,
    pub placeholder_text: Color,
}

impl ThemePalette {
    pub fn risk_color(&self, level: RiskLevel) -> Color {
        match level {
            RiskLevel::Low => self.status_warning,
            RiskLevel::Medium => self.status_medium_risk,
            RiskLevel::High => self.status_error,
        }
    }

    pub fn modal_title_style(&self) -> Style {
        Style::default()
            .fg(self.modal_title)
            .add_modifier(Modifier::BOLD)
    }

    pub fn modal_hint_style(&self) -> Style {
        Style::default()
            .fg(self.modal_title)
            .add_modifier(Modifier::BOLD)
    }

    pub fn panel_border_style(&self, focused: bool, highlight: bool) -> Style {
        let color = if focused {
            self.focus_border
        } else if highlight {
            self.highlight_border
        } else {
            self.unfocus_border
        };
        Style::default().fg(color)
    }

    pub fn picker_selected_style(&self) -> Style {
        Style::default()
            .bg(self.completion_selected_bg)
            .fg(self.text_primary)
            .add_modifier(Modifier::BOLD)
    }

    pub fn input_border_style(&self, focused: bool, has_error: bool) -> Style {
        let color = if has_error {
            self.status_error
        } else if focused {
            self.modal_border_highlight
        } else {
            self.modal_border
        };
        Style::default().fg(color)
    }

    pub fn status_style(&self, tone: StatusTone) -> Style {
        let color = match tone {
            StatusTone::Success => self.status_success,
            StatusTone::Error => self.status_error,
            StatusTone::Warning => self.status_warning,
        };
        Style::default().fg(color)
    }

    pub fn cursor_style(&self) -> Style {
        Style::default().bg(self.cursor_fg).fg(self.selection_bg)
    }
}

pub const DEFAULT_THEME: ThemePalette = ThemePalette {
    modal_border: Color::Rgb(0x45, 0x47, 0x55),
    modal_border_highlight: Color::Rgb(0xb0, 0xb4, 0xbe),
    modal_title: Color::Rgb(0xc9, 0xce, 0xd8),
    modal_hint: Color::Rgb(0x5b, 0x5f, 0x6e),
    key_chip_bg: Color::Rgb(0x3a, 0x3a, 0x4a),
    key_chip_fg: Color::Rgb(0xee, 0xcc, 0x66),
    editor_current_line_bg: Color::Rgb(0x22, 0x26, 0x33),
    completion_selected_bg: Color::Rgb(0x45, 0x47, 0x5a),
    input_value: Color::Rgb(0xaa, 0xaa, 0xaa),
    note_text: Color::Rgb(0x66, 0x66, 0x77),
    focus_border: Color::Rgb(0x97, 0xc9, 0xc3),
    unfocus_border: Color::Rgb(0x45, 0x47, 0x55),
    highlight_border: Color::Rgb(0xb0, 0xdd, 0xd8),
    text_primary: Color::Rgb(0xc9, 0xce, 0xd8),
    text_secondary: Color::Rgb(0xb0, 0xb4, 0xbe),
    text_muted: Color::Rgb(0x5b, 0x5f, 0x6e),
    text_dim: Color::Rgb(0x77, 0x77, 0x88),
    text_accent: Color::Rgb(0xc4, 0xb2, 0x8a),
    status_success: Color::Rgb(0x97, 0xc9, 0xc3),
    status_error: Color::Rgb(0xc4, 0x74, 0x6e),
    status_warning: Color::Rgb(0xc4, 0xb2, 0x8a),
    status_medium_risk: Color::Rgb(0xff, 0x99, 0x00),
    cursor_fg: Color::White,
    section_header: Color::Rgb(0x97, 0xc9, 0xc3),
    scrollbar_active: Color::Rgb(0x6a, 0x9e, 0x98),
    scrollbar_inactive: Color::Rgb(0x45, 0x47, 0x55),
    result_row_active_bg: Color::Rgb(0x2e, 0x2e, 0x44),
    result_cell_active_bg: Color::Rgb(0x3a, 0x3a, 0x5a),
    cell_edit_fg: Color::Rgb(0xc4, 0xb2, 0x8a),
    cell_draft_pending_fg: Color::Rgb(0xff, 0x99, 0x00),
    staged_delete_bg: Color::Rgb(0x3d, 0x22, 0x22),
    staged_delete_fg: Color::Rgb(0xee, 0x77, 0x77),
    yank_flash_bg: Color::Rgb(0xF4, 0x9E, 0x4C),
    yank_flash_fg: Color::Rgb(0x11, 0x14, 0x19),
    sql_keyword: Color::Rgb(0x89, 0xb4, 0xfa),
    sql_string: Color::Rgb(0xa6, 0xe3, 0xa1),
    sql_number: Color::Rgb(0xfa, 0xb3, 0x87),
    sql_comment: Color::Rgb(0x6c, 0x70, 0x86),
    sql_operator: Color::Rgb(0x94, 0xe2, 0xd5),
    sql_text: Color::Rgb(0xc9, 0xce, 0xd8),
    json_key: Color::Rgb(0x97, 0xc9, 0xc3),
    json_string: Color::Rgb(0x8a, 0xb8, 0x8a),
    json_number: Color::Rgb(0xc8, 0x9b, 0x7a),
    json_bool: Color::Rgb(0xc8, 0x9b, 0x7a),
    json_null: Color::Rgb(0x5b, 0x5f, 0x6e),
    json_bracket: Color::Rgb(0xb0, 0xb4, 0xbe),
    striped_row_bg: Color::Rgb(0x1e, 0x1e, 0x23),
    selection_bg: Color::Black,
    tab_active: Color::Rgb(0x97, 0xc9, 0xc3),
    tab_inactive: Color::Rgb(0x5b, 0x5f, 0x6e),
    active_indicator: Color::Rgb(0x97, 0xc9, 0xc3),
    inactive_indicator: Color::Rgb(0x5b, 0x5f, 0x6e),
    placeholder_text: Color::Rgb(0x5b, 0x5f, 0x6e),
};

pub fn palette_for(theme_id: ThemeId) -> &'static ThemePalette {
    match theme_id {
        ThemeId::Default => &DEFAULT_THEME,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_for_default_returns_default_theme() {
        assert_eq!(palette_for(ThemeId::Default), &DEFAULT_THEME);
    }

    #[test]
    fn panel_border_style_prefers_focus_over_highlight() {
        let style = DEFAULT_THEME.panel_border_style(true, true);

        assert_eq!(style.fg, Some(DEFAULT_THEME.focus_border));
    }

    #[test]
    fn picker_selected_style_uses_selected_colors() {
        let style = DEFAULT_THEME.picker_selected_style();

        assert_eq!(style.bg, Some(DEFAULT_THEME.completion_selected_bg));
        assert_eq!(style.fg, Some(DEFAULT_THEME.text_primary));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn input_border_style_prefers_error_over_focus() {
        let style = DEFAULT_THEME.input_border_style(true, true);

        assert_eq!(style.fg, Some(DEFAULT_THEME.status_error));
    }

    #[test]
    fn status_style_uses_requested_tone() {
        let style = DEFAULT_THEME.status_style(StatusTone::Warning);

        assert_eq!(style.fg, Some(DEFAULT_THEME.status_warning));
    }

    #[test]
    fn cursor_style_inverts_cursor_and_selection_colors() {
        let style = DEFAULT_THEME.cursor_style();

        assert_eq!(style.bg, Some(DEFAULT_THEME.cursor_fg));
        assert_eq!(style.fg, Some(DEFAULT_THEME.selection_bg));
    }
}
