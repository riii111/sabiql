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
            .fg(self.semantic.text.primary)
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
            focus_border: Color::Rgb(0x97, 0xc9, 0xc3),
            unfocus_border: Color::Rgb(0x4b, 0x55, 0x63),
            highlight_border: Color::Rgb(0xb0, 0xdd, 0xd8),
        },
        text: TextTokens {
            primary: Color::Rgb(0xe5, 0xe7, 0xeb),
            secondary: Color::Rgb(0xc7, 0xd0, 0xda),
            muted: Color::Rgb(0x6b, 0x78, 0x8a),
            dim: Color::Rgb(0x7b, 0x84, 0x95),
            accent: Color::Rgb(0xd4, 0xa4, 0x85),
            placeholder: Color::Rgb(0x6b, 0x78, 0x8a),
        },
        status: StatusTokens {
            success: Color::Rgb(0x2d, 0xc4, 0x92),
            error: Color::Rgb(0xff, 0x5c, 0x57),
            warning: Color::Rgb(0xf3, 0xb3, 0x45),
            pending: Color::Rgb(0xf5, 0x9e, 0x0b),
            medium_risk: Color::Rgb(0xff, 0x7a, 0x45),
        },
        cursor: CursorTokens {
            fg: Color::White,
            bg: Color::White,
            text_fg: Color::Black,
        },
    },
    component: ComponentTokens {
        modal: ModalTokens {
            title: Color::Rgb(0xe5, 0xe7, 0xeb),
            hint: Color::Rgb(0xc7, 0xd0, 0xda),
            border: Color::Rgb(0x60, 0x68, 0x76),
            border_highlight: Color::Rgb(0xa7, 0xb4, 0xc3),
        },
        navigation: NavigationTokens {
            key_chip_bg: Color::Rgb(0x30, 0x35, 0x45),
            key_chip_fg: Color::Rgb(0xd4, 0xa4, 0x85),
            section_header: Color::Rgb(0x6a, 0xb8, 0x9a),
            scrollbar_active: Color::Rgb(0xa7, 0xb4, 0xc3),
            scrollbar_inactive: Color::Rgb(0x4b, 0x55, 0x63),
            tab_active: Color::Rgb(0xd9, 0xb5, 0x6f),
            tab_inactive: Color::Rgb(0x6b, 0x78, 0x8a),
            active_indicator: Color::Rgb(0xf8, 0xfa, 0xfc),
        },
        editor: EditorTokens {
            current_line_bg: Color::Rgb(0x20, 0x26, 0x33),
            completion_selected_bg: Color::Rgb(0x34, 0x45, 0x5a),
        },
        table: TableTokens {
            result_row_active_bg: Color::Rgb(0x27, 0x35, 0x4a),
            result_cell_active_bg: Color::Rgb(0x34, 0x45, 0x5f),
            cell_edit_fg: Color::Rgb(0x9e, 0xd8, 0xd0),
            staged_delete_bg: Color::Rgb(0x46, 0x1e, 0x24),
            staged_delete_fg: Color::Rgb(0xff, 0x8a, 0x86),
            striped_row_bg: Color::Rgb(0x1d, 0x20, 0x27),
        },
        feedback: FeedbackTokens {
            yank_flash_bg: Color::Rgb(0xf5, 0x9e, 0x0b),
            yank_flash_fg: Color::Rgb(0x11, 0x14, 0x19),
            note_text: Color::Rgb(0x6b, 0x78, 0x8a),
        },
        syntax: SyntaxTokens {
            sql_keyword: Color::Rgb(0x8a, 0xb4, 0xf8),
            sql_string: Color::Rgb(0x9e, 0xd8, 0x9a),
            sql_number: Color::Rgb(0xf2, 0x8c, 0x4b),
            sql_comment: Color::Rgb(0x7b, 0x84, 0x95),
            sql_operator: Color::Rgb(0x8b, 0xf0, 0xe3),
            sql_text: Color::Rgb(0xe5, 0xe7, 0xeb),
        },
    },
};

pub const LIGHT_THEME: ThemePalette = ThemePalette {
    semantic: SemanticTokens {
        surface: SurfaceTokens {
            focus_border: Color::Rgb(0x1d, 0x6f, 0x68),
            unfocus_border: Color::Rgb(0xb8, 0xc0, 0xc8),
            highlight_border: Color::Rgb(0x0f, 0x7d, 0x74),
        },
        text: TextTokens {
            primary: Color::Rgb(0x1f, 0x25, 0x2e),
            secondary: Color::Rgb(0x4b, 0x55, 0x63),
            muted: Color::Rgb(0x75, 0x7f, 0x8c),
            dim: Color::Rgb(0x8a, 0x93, 0xa0),
            accent: Color::Rgb(0x9a, 0x4f, 0x1d),
            placeholder: Color::Rgb(0x8a, 0x93, 0xa0),
        },
        status: StatusTokens {
            success: Color::Rgb(0x0c, 0x7a, 0x4b),
            error: Color::Rgb(0xb4, 0x23, 0x18),
            warning: Color::Rgb(0x9a, 0x62, 0x00),
            pending: Color::Rgb(0xa3, 0x5f, 0x00),
            medium_risk: Color::Rgb(0xc2, 0x41, 0x0c),
        },
        cursor: CursorTokens {
            fg: Color::Rgb(0x1d, 0x6f, 0x68),
            bg: Color::Rgb(0x1d, 0x6f, 0x68),
            text_fg: Color::Rgb(0xff, 0xff, 0xff),
        },
    },
    component: ComponentTokens {
        modal: ModalTokens {
            title: Color::Rgb(0x1f, 0x25, 0x2e),
            hint: Color::Rgb(0x4b, 0x55, 0x63),
            border: Color::Rgb(0xa8, 0xb0, 0xbb),
            border_highlight: Color::Rgb(0x1d, 0x6f, 0x68),
        },
        navigation: NavigationTokens {
            key_chip_bg: Color::Rgb(0xe7, 0xeb, 0xf0),
            key_chip_fg: Color::Rgb(0x8a, 0x45, 0x18),
            section_header: Color::Rgb(0x0f, 0x6f, 0x63),
            scrollbar_active: Color::Rgb(0x1d, 0x6f, 0x68),
            scrollbar_inactive: Color::Rgb(0xc9, 0xcf, 0xd8),
            tab_active: Color::Rgb(0x1d, 0x6f, 0x68),
            tab_inactive: Color::Rgb(0x75, 0x7f, 0x8c),
            active_indicator: Color::Rgb(0x1d, 0x6f, 0x68),
        },
        editor: EditorTokens {
            current_line_bg: Color::Rgb(0xec, 0xf2, 0xf2),
            completion_selected_bg: Color::Rgb(0xd8, 0xe9, 0xe7),
        },
        table: TableTokens {
            result_row_active_bg: Color::Rgb(0xde, 0xe9, 0xf3),
            result_cell_active_bg: Color::Rgb(0xc9, 0xdc, 0xea),
            cell_edit_fg: Color::Rgb(0x0f, 0x6f, 0x63),
            staged_delete_bg: Color::Rgb(0xf6, 0xde, 0xda),
            staged_delete_fg: Color::Rgb(0xb4, 0x23, 0x18),
            striped_row_bg: Color::Rgb(0xf2, 0xf4, 0xf7),
        },
        feedback: FeedbackTokens {
            yank_flash_bg: Color::Rgb(0xff, 0xd2, 0x73),
            yank_flash_fg: Color::Rgb(0x1f, 0x25, 0x2e),
            note_text: Color::Rgb(0x6b, 0x72, 0x80),
        },
        syntax: SyntaxTokens {
            sql_keyword: Color::Rgb(0x1b, 0x5e, 0xa8),
            sql_string: Color::Rgb(0x0c, 0x7a, 0x4b),
            sql_number: Color::Rgb(0x9a, 0x4f, 0x1d),
            sql_comment: Color::Rgb(0x75, 0x7f, 0x8c),
            sql_operator: Color::Rgb(0x5b, 0x5f, 0xa8),
            sql_text: Color::Rgb(0x1f, 0x25, 0x2e),
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
        ThemeId::Light => &LIGHT_THEME,
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
    fn palette_for_light_returns_light_theme() {
        assert_eq!(palette_for(ThemeId::Light), &LIGHT_THEME);
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
        assert_eq!(style.fg, Some(DEFAULT_THEME.semantic.text.primary));
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
