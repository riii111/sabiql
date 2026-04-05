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
    pub cursor_bg: Color,
    pub cursor_text_fg: Color,
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
        Style::default().fg(self.modal_hint)
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
        Style::default().bg(self.cursor_bg).fg(self.cursor_text_fg)
    }
}

pub const DEFAULT_THEME: ThemePalette = ThemePalette {
    modal_border: Color::Rgb(0x70, 0x68, 0x60),
    modal_border_highlight: Color::Rgb(0xc0, 0xb8, 0xb8),
    modal_title: Color::Rgb(0xe9, 0xdb, 0xdb),
    modal_hint: Color::Rgb(0xc0, 0xb8, 0xb0),
    key_chip_bg: Color::Rgb(0x3a, 0x3a, 0x4a),
    key_chip_fg: Color::Rgb(0xd4, 0xa4, 0x85),
    editor_current_line_bg: Color::Rgb(0x22, 0x26, 0x33),
    completion_selected_bg: Color::Rgb(0x45, 0x47, 0x5a),
    input_value: Color::Rgb(0xaa, 0xaa, 0xaa),
    note_text: Color::Rgb(0x66, 0x66, 0x77),
    focus_border: Color::Rgb(0x97, 0xc9, 0xc3),
    unfocus_border: Color::Rgb(0x45, 0x47, 0x55),
    highlight_border: Color::Rgb(0xb0, 0xdd, 0xd8),
    text_primary: Color::Rgb(0xe9, 0xdb, 0xdb),
    text_secondary: Color::Rgb(0xc0, 0xb8, 0xb8),
    text_muted: Color::Rgb(0x5c, 0x63, 0x70),
    text_dim: Color::Rgb(0x6a, 0x6e, 0x7a),
    text_accent: Color::Rgb(0xd4, 0xa4, 0x85),
    status_success: Color::Rgb(0x97, 0xc9, 0xc3),
    status_error: Color::Rgb(0xc4, 0x74, 0x6e),
    status_warning: Color::Rgb(0xe0, 0xaf, 0x68),
    status_medium_risk: Color::Rgb(0xd4, 0x70, 0x50),
    cursor_fg: Color::White,
    cursor_bg: Color::White,
    cursor_text_fg: Color::Black,
    section_header: Color::Rgb(0x6a, 0xb8, 0x9a),
    scrollbar_active: Color::Rgb(0xc0, 0xb8, 0xb0),
    scrollbar_inactive: Color::Rgb(0x50, 0x52, 0x5e),
    result_row_active_bg: Color::Rgb(0x2e, 0x2e, 0x44),
    result_cell_active_bg: Color::Rgb(0x3a, 0x3a, 0x5a),
    cell_edit_fg: Color::Rgb(0xa8, 0xb8, 0xb5),
    cell_draft_pending_fg: Color::Rgb(0xd4, 0xa0, 0x60),
    staged_delete_bg: Color::Rgb(0x3d, 0x22, 0x22),
    staged_delete_fg: Color::Rgb(0xee, 0x77, 0x77),
    yank_flash_bg: Color::Rgb(0xF4, 0x9E, 0x4C),
    yank_flash_fg: Color::Rgb(0x11, 0x14, 0x19),
    sql_keyword: Color::Rgb(0x80, 0x90, 0xa8),
    sql_string: Color::Rgb(0xcd, 0xc8, 0xdb),
    sql_number: Color::Rgb(0xd4, 0xa4, 0x85),
    sql_comment: Color::Rgb(0x62, 0x72, 0xa4),
    sql_operator: Color::Rgb(0x8a, 0x91, 0xa5),
    sql_text: Color::Rgb(0xe9, 0xdb, 0xdb),
    json_key: Color::Rgb(0x7a, 0x9f, 0xc8),
    json_string: Color::Rgb(0x8a, 0xb8, 0x8a),
    json_number: Color::Rgb(0xb0, 0x9a, 0x88),
    json_bool: Color::Rgb(0xb0, 0x9a, 0x88),
    json_null: Color::Rgb(0x5b, 0x5f, 0x6e),
    json_bracket: Color::Rgb(0xc0, 0xb8, 0xb8),
    striped_row_bg: Color::Rgb(0x1e, 0x1e, 0x23),
    tab_active: Color::Rgb(0xd0, 0xc0, 0xa0),
    tab_inactive: Color::Rgb(0x5b, 0x5f, 0x6e),
    active_indicator: Color::Rgb(0xff, 0xff, 0xff),
    inactive_indicator: Color::Rgb(0x5b, 0x5f, 0x6e),
    placeholder_text: Color::Rgb(0x5b, 0x5f, 0x6e),
};

#[cfg(any(test, feature = "test-support"))]
pub const TEST_CONTRAST_THEME: ThemePalette = ThemePalette {
    modal_border: Color::Rgb(0xd8, 0x2a, 0x1f),
    modal_border_highlight: Color::Rgb(0xff, 0xe0, 0x66),
    modal_title: Color::Rgb(0xf6, 0xf0, 0xe8),
    modal_hint: Color::Rgb(0x7b, 0xe0, 0x73),
    key_chip_bg: Color::Rgb(0x1a, 0x45, 0x5e),
    key_chip_fg: Color::Rgb(0xff, 0xe0, 0x66),
    editor_current_line_bg: Color::Rgb(0x1d, 0x2d, 0x3f),
    completion_selected_bg: Color::Rgb(0x2d, 0x5d, 0x46),
    input_value: Color::Rgb(0xf6, 0xf0, 0xe8),
    note_text: Color::Rgb(0x92, 0xb3, 0xc2),
    focus_border: Color::Rgb(0x2f, 0xc4, 0xb2),
    unfocus_border: Color::Rgb(0x5d, 0x62, 0x74),
    highlight_border: Color::Rgb(0xff, 0xc8, 0x57),
    text_primary: Color::Rgb(0xf6, 0xf0, 0xe8),
    text_secondary: Color::Rgb(0xc9, 0xd6, 0xdf),
    text_muted: Color::Rgb(0x92, 0xb3, 0xc2),
    text_dim: Color::Rgb(0x6a, 0x85, 0x95),
    text_accent: Color::Rgb(0xff, 0xc8, 0x57),
    status_success: Color::Rgb(0x7b, 0xe0, 0x73),
    status_error: Color::Rgb(0xff, 0x7a, 0x59),
    status_warning: Color::Rgb(0xff, 0xc8, 0x57),
    status_medium_risk: Color::Rgb(0xff, 0x9f, 0x1c),
    cursor_fg: Color::Rgb(0xff, 0xf4, 0xe0),
    cursor_bg: Color::Rgb(0xff, 0xf4, 0xe0),
    cursor_text_fg: Color::Rgb(0x0d, 0x11, 0x18),
    section_header: Color::Rgb(0x2f, 0xc4, 0xb2),
    scrollbar_active: Color::Rgb(0x2f, 0xc4, 0xb2),
    scrollbar_inactive: Color::Rgb(0x5d, 0x62, 0x74),
    result_row_active_bg: Color::Rgb(0x2b, 0x32, 0x54),
    result_cell_active_bg: Color::Rgb(0x3a, 0x44, 0x6e),
    cell_edit_fg: Color::Rgb(0xff, 0xe0, 0x66),
    cell_draft_pending_fg: Color::Rgb(0xff, 0x9f, 0x1c),
    staged_delete_bg: Color::Rgb(0x4a, 0x1f, 0x1f),
    staged_delete_fg: Color::Rgb(0xff, 0x7a, 0x59),
    yank_flash_bg: Color::Rgb(0xff, 0xc8, 0x57),
    yank_flash_fg: Color::Rgb(0x14, 0x17, 0x21),
    sql_keyword: Color::Rgb(0x7d, 0xc4, 0xff),
    sql_string: Color::Rgb(0x9b, 0xf0, 0x8f),
    sql_number: Color::Rgb(0xff, 0xb8, 0x6b),
    sql_comment: Color::Rgb(0x7c, 0x8a, 0xa5),
    sql_operator: Color::Rgb(0x5e, 0xe0, 0xd5),
    sql_text: Color::Rgb(0xf6, 0xf0, 0xe8),
    json_key: Color::Rgb(0x2f, 0xc4, 0xb2),
    json_string: Color::Rgb(0x9b, 0xf0, 0x8f),
    json_number: Color::Rgb(0xff, 0xb8, 0x6b),
    json_bool: Color::Rgb(0xff, 0xb8, 0x6b),
    json_null: Color::Rgb(0x92, 0xb3, 0xc2),
    json_bracket: Color::Rgb(0xc9, 0xd6, 0xdf),
    striped_row_bg: Color::Rgb(0x1d, 0x21, 0x2b),
    tab_active: Color::Rgb(0x2f, 0xc4, 0xb2),
    tab_inactive: Color::Rgb(0x92, 0xb3, 0xc2),
    active_indicator: Color::Rgb(0x2f, 0xc4, 0xb2),
    inactive_indicator: Color::Rgb(0x92, 0xb3, 0xc2),
    placeholder_text: Color::Rgb(0x92, 0xb3, 0xc2),
};

pub fn palette_for(theme_id: ThemeId) -> &'static ThemePalette {
    match theme_id {
        ThemeId::Default => &DEFAULT_THEME,
        #[cfg(any(test, feature = "test-support"))]
        ThemeId::TestContrast => &TEST_CONTRAST_THEME,
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
    fn palette_for_test_contrast_returns_test_theme() {
        assert_eq!(palette_for(ThemeId::TestContrast), &TEST_CONTRAST_THEME);
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
    fn modal_hint_style_uses_hint_token_without_bold() {
        let style = DEFAULT_THEME.modal_hint_style();

        assert_eq!(style.fg, Some(DEFAULT_THEME.modal_hint));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn cursor_style_inverts_cursor_and_selection_colors() {
        let style = DEFAULT_THEME.cursor_style();

        assert_eq!(style.bg, Some(DEFAULT_THEME.cursor_bg));
        assert_eq!(style.fg, Some(DEFAULT_THEME.cursor_text_fg));
    }
}
