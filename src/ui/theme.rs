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
    pub semantic: SemanticTokens,
    pub component: ComponentTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticTokens {
    pub surface: SurfaceTokens,
    pub text: TextTokens,
    pub status: StatusTokens,
    pub cursor: CursorTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceTokens {
    pub focus_border: Color,
    pub unfocus_border: Color,
    pub highlight_border: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextTokens {
    pub primary: Color,
    pub secondary: Color,
    pub muted: Color,
    pub dim: Color,
    pub accent: Color,
    pub placeholder: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusTokens {
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub pending: Color,
    pub medium_risk: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorTokens {
    pub fg: Color,
    pub bg: Color,
    pub text_fg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentTokens {
    pub modal: ModalTokens,
    pub navigation: NavigationTokens,
    pub editor: EditorTokens,
    pub table: TableTokens,
    pub feedback: FeedbackTokens,
    pub syntax: SyntaxTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModalTokens {
    pub title: Color,
    pub hint: Color,
    pub border: Color,
    pub border_highlight: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationTokens {
    pub key_chip_bg: Color,
    pub key_chip_fg: Color,
    pub section_header: Color,
    pub scrollbar_active: Color,
    pub scrollbar_inactive: Color,
    pub tab_active: Color,
    pub tab_inactive: Color,
    pub active_indicator: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorTokens {
    pub current_line_bg: Color,
    pub completion_selected_bg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableTokens {
    pub result_row_active_bg: Color,
    pub result_cell_active_bg: Color,
    pub cell_edit_fg: Color,
    pub staged_delete_bg: Color,
    pub staged_delete_fg: Color,
    pub striped_row_bg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedbackTokens {
    pub yank_flash_bg: Color,
    pub yank_flash_fg: Color,
    pub note_text: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxTokens {
    pub sql_keyword: Color,
    pub sql_string: Color,
    pub sql_number: Color,
    pub sql_comment: Color,
    pub sql_operator: Color,
    pub sql_text: Color,
}

impl ThemePalette {
    pub fn risk_color(&self, level: RiskLevel) -> Color {
        match level {
            RiskLevel::Low => self.semantic.status.warning,
            RiskLevel::Medium => self.semantic.status.medium_risk,
            RiskLevel::High => self.semantic.status.error,
        }
    }

    pub fn modal_title_style(&self) -> Style {
        Style::default()
            .fg(self.component.modal.title)
            .add_modifier(Modifier::BOLD)
    }

    pub fn modal_hint_style(&self) -> Style {
        Style::default().fg(self.component.modal.hint)
    }

    pub fn panel_border_style(&self, focused: bool, highlight: bool) -> Style {
        let color = if focused {
            self.semantic.surface.focus_border
        } else if highlight {
            self.semantic.surface.highlight_border
        } else {
            self.semantic.surface.unfocus_border
        };
        Style::default().fg(color)
    }

    pub fn picker_selected_style(&self) -> Style {
        Style::default()
            .bg(self.component.editor.completion_selected_bg)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }

    pub fn modal_input_border_style(&self, focused: bool, has_error: bool) -> Style {
        let color = if has_error {
            self.semantic.status.error
        } else if focused {
            self.component.modal.border_highlight
        } else {
            self.component.modal.border
        };
        Style::default().fg(color)
    }

    pub fn modal_border_style(&self) -> Style {
        Style::default().fg(self.component.modal.border)
    }

    pub fn status_style(&self, tone: StatusTone) -> Style {
        let color = match tone {
            StatusTone::Success => self.semantic.status.success,
            StatusTone::Error => self.semantic.status.error,
            StatusTone::Warning => self.semantic.status.warning,
        };
        Style::default().fg(color)
    }

    pub fn block_cursor_style(&self) -> Style {
        Style::default()
            .bg(self.semantic.cursor.bg)
            .fg(self.semantic.cursor.text_fg)
    }

    pub fn insert_cursor_style(&self) -> Style {
        Style::default().fg(self.semantic.cursor.fg)
    }
}

pub const DEFAULT_THEME: ThemePalette = ThemePalette {
    semantic: SemanticTokens {
        surface: SurfaceTokens {
            focus_border: Color::Blue,
            unfocus_border: Color::DarkGray,
            highlight_border: Color::Yellow,
        },
        text: TextTokens {
            primary: Color::Reset,
            secondary: Color::Gray,
            muted: Color::DarkGray,
            dim: Color::DarkGray,
            accent: Color::Blue,
            placeholder: Color::DarkGray,
        },
        status: StatusTokens {
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            pending: Color::Yellow,
            medium_risk: Color::Magenta,
        },
        cursor: CursorTokens {
            fg: Color::Blue,
            bg: Color::Blue,
            text_fg: Color::White,
        },
    },
    component: ComponentTokens {
        modal: ModalTokens {
            title: Color::Reset,
            hint: Color::Blue,
            border: Color::DarkGray,
            border_highlight: Color::Blue,
        },
        navigation: NavigationTokens {
            key_chip_bg: Color::Blue,
            key_chip_fg: Color::White,
            section_header: Color::Blue,
            scrollbar_active: Color::Blue,
            scrollbar_inactive: Color::DarkGray,
            tab_active: Color::Blue,
            tab_inactive: Color::DarkGray,
            active_indicator: Color::Blue,
        },
        editor: EditorTokens {
            current_line_bg: Color::Reset,
            completion_selected_bg: Color::Blue,
        },
        table: TableTokens {
            result_row_active_bg: Color::Blue,
            result_cell_active_bg: Color::Blue,
            cell_edit_fg: Color::White,
            staged_delete_bg: Color::Red,
            staged_delete_fg: Color::White,
            striped_row_bg: Color::Reset,
        },
        feedback: FeedbackTokens {
            yank_flash_bg: Color::Yellow,
            yank_flash_fg: Color::Black,
            note_text: Color::DarkGray,
        },
        syntax: SyntaxTokens {
            sql_keyword: Color::Blue,
            sql_string: Color::Green,
            sql_number: Color::Magenta,
            sql_comment: Color::DarkGray,
            sql_operator: Color::Cyan,
            sql_text: Color::Reset,
        },
    },
};

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub const TEST_CONTRAST_THEME: ThemePalette = ThemePalette {
    semantic: SemanticTokens {
        surface: SurfaceTokens {
            focus_border: Color::Rgb(0x2f, 0xc4, 0xb2),
            unfocus_border: Color::Rgb(0x5d, 0x62, 0x74),
            highlight_border: Color::Rgb(0xff, 0xc8, 0x57),
        },
        text: TextTokens {
            primary: Color::Rgb(0xf6, 0xf0, 0xe8),
            secondary: Color::Rgb(0xc9, 0xd6, 0xdf),
            muted: Color::Rgb(0x92, 0xb3, 0xc2),
            dim: Color::Rgb(0x6a, 0x85, 0x95),
            accent: Color::Rgb(0xff, 0xc8, 0x57),
            placeholder: Color::Rgb(0x92, 0xb3, 0xc2),
        },
        status: StatusTokens {
            success: Color::Rgb(0x7b, 0xe0, 0x73),
            error: Color::Rgb(0xff, 0x7a, 0x59),
            warning: Color::Rgb(0xff, 0xc8, 0x57),
            pending: Color::Rgb(0xff, 0x9f, 0x1c),
            medium_risk: Color::Rgb(0xff, 0x9f, 0x1c),
        },
        cursor: CursorTokens {
            fg: Color::Rgb(0xff, 0xf4, 0xe0),
            bg: Color::Rgb(0xff, 0xf4, 0xe0),
            text_fg: Color::Rgb(0x0d, 0x11, 0x18),
        },
    },
    component: ComponentTokens {
        modal: ModalTokens {
            title: Color::Rgb(0xf6, 0xf0, 0xe8),
            hint: Color::Rgb(0x7b, 0xe0, 0x73),
            border: Color::Rgb(0xd8, 0x2a, 0x1f),
            border_highlight: Color::Rgb(0xff, 0xe0, 0x66),
        },
        navigation: NavigationTokens {
            key_chip_bg: Color::Rgb(0x1a, 0x45, 0x5e),
            key_chip_fg: Color::Rgb(0xff, 0xe0, 0x66),
            section_header: Color::Rgb(0x2f, 0xc4, 0xb2),
            scrollbar_active: Color::Rgb(0x2f, 0xc4, 0xb2),
            scrollbar_inactive: Color::Rgb(0x5d, 0x62, 0x74),
            tab_active: Color::Rgb(0x2f, 0xc4, 0xb2),
            tab_inactive: Color::Rgb(0x92, 0xb3, 0xc2),
            active_indicator: Color::Rgb(0x2f, 0xc4, 0xb2),
        },
        editor: EditorTokens {
            current_line_bg: Color::Rgb(0x1d, 0x2d, 0x3f),
            completion_selected_bg: Color::Rgb(0x2d, 0x5d, 0x46),
        },
        table: TableTokens {
            result_row_active_bg: Color::Rgb(0x2b, 0x32, 0x54),
            result_cell_active_bg: Color::Rgb(0x3a, 0x44, 0x6e),
            cell_edit_fg: Color::Rgb(0xff, 0xe0, 0x66),
            staged_delete_bg: Color::Rgb(0x4a, 0x1f, 0x1f),
            staged_delete_fg: Color::Rgb(0xff, 0x7a, 0x59),
            striped_row_bg: Color::Rgb(0x1d, 0x21, 0x2b),
        },
        feedback: FeedbackTokens {
            yank_flash_bg: Color::Rgb(0xff, 0xc8, 0x57),
            yank_flash_fg: Color::Rgb(0x14, 0x17, 0x21),
            note_text: Color::Rgb(0x92, 0xb3, 0xc2),
        },
        syntax: SyntaxTokens {
            sql_keyword: Color::Rgb(0x7d, 0xc4, 0xff),
            sql_string: Color::Rgb(0x9b, 0xf0, 0x8f),
            sql_number: Color::Rgb(0xff, 0xb8, 0x6b),
            sql_comment: Color::Rgb(0x7c, 0x8a, 0xa5),
            sql_operator: Color::Rgb(0x5e, 0xe0, 0xd5),
            sql_text: Color::Rgb(0xf6, 0xf0, 0xe8),
        },
    },
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

        assert_eq!(style.fg, Some(DEFAULT_THEME.semantic.surface.focus_border));
    }

    #[test]
    fn picker_selected_style_uses_selected_colors() {
        let style = DEFAULT_THEME.picker_selected_style();

        assert_eq!(
            style.bg,
            Some(DEFAULT_THEME.component.editor.completion_selected_bg)
        );
        assert_eq!(style.fg, Some(Color::White));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn modal_input_border_style_prefers_error_over_focus() {
        let style = DEFAULT_THEME.modal_input_border_style(true, true);

        assert_eq!(style.fg, Some(DEFAULT_THEME.semantic.status.error));
    }

    #[test]
    fn status_style_uses_requested_tone() {
        let style = DEFAULT_THEME.status_style(StatusTone::Warning);

        assert_eq!(style.fg, Some(DEFAULT_THEME.semantic.status.warning));
    }

    #[test]
    fn modal_hint_style_uses_hint_token_without_bold() {
        let style = DEFAULT_THEME.modal_hint_style();

        assert_eq!(style.fg, Some(DEFAULT_THEME.component.modal.hint));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn modal_border_style_uses_component_modal_border() {
        let style = DEFAULT_THEME.modal_border_style();

        assert_eq!(style.fg, Some(DEFAULT_THEME.component.modal.border));
    }

    #[test]
    fn block_cursor_style_uses_semantic_cursor_colors() {
        let style = DEFAULT_THEME.block_cursor_style();

        assert_eq!(style.bg, Some(DEFAULT_THEME.semantic.cursor.bg));
        assert_eq!(style.fg, Some(DEFAULT_THEME.semantic.cursor.text_fg));
    }
}
