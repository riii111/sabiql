use ratatui::style::Color;

use crate::app::policy::write::write_guardrails::RiskLevel;

pub struct Palette;

impl Palette {
    pub const CATPPUCCIN_MOCHA: Color = Color::Rgb(0x1e, 0x1e, 0x2e);
}

pub struct Theme;

impl Theme {
    // Modal border
    pub const MODAL_BORDER: Color = Color::DarkGray;
    pub const MODAL_BORDER_HIGHLIGHT: Color = Color::Gray;

    // Modal title (emphasized)
    pub const MODAL_TITLE: Color = Color::White;

    // Modal hint text (de-emphasized)
    pub const MODAL_HINT: Color = Color::DarkGray;

    // Key chip (for important keys in Help)
    pub const KEY_CHIP_BG: Color = Color::Rgb(0x3a, 0x3a, 0x4a);
    pub const KEY_CHIP_FG: Color = Color::Rgb(0xee, 0xcc, 0x66);

    // SQL Editor current line highlight
    pub const EDITOR_CURRENT_LINE_BG: Color = Color::Rgb(0x22, 0x26, 0x33);

    // Completion popup
    pub const COMPLETION_SELECTED_BG: Color = Color::Rgb(0x45, 0x47, 0x5a);

    // Form input values (non-focused, readable against dark bg)
    pub const INPUT_VALUE: Color = Color::Rgb(0xaa, 0xaa, 0xaa);

    // Note/disclaimer text (subtle but readable)
    pub const NOTE_TEXT: Color = Color::Rgb(0x66, 0x66, 0x77);

    // ============ Panel/Border Colors ============

    // Panel border states
    pub const FOCUS_BORDER: Color = Color::Cyan;
    pub const UNFOCUS_BORDER: Color = Color::DarkGray;
    pub const HIGHLIGHT_BORDER: Color = Color::Green;

    // ============ Text Colors ============

    // Semantic text colors
    pub const TEXT_PRIMARY: Color = Color::White;
    pub const TEXT_SECONDARY: Color = Color::Gray;
    pub const TEXT_MUTED: Color = Color::DarkGray;
    pub const TEXT_DIM: Color = Color::Rgb(0x77, 0x77, 0x88);
    pub const TEXT_ACCENT: Color = Color::Yellow;

    // ============ Status Colors ============

    // Status indicators
    pub const STATUS_SUCCESS: Color = Color::Green;
    pub const STATUS_ERROR: Color = Color::Red;
    pub const STATUS_WARNING: Color = Color::Yellow;
    pub const STATUS_MEDIUM_RISK: Color = Color::Rgb(0xff, 0x99, 0x00);

    // ============ Component Colors ============

    // Cursor
    pub const CURSOR_FG: Color = Color::White;

    // Section headers
    pub const SECTION_HEADER: Color = Color::Cyan;

    // Scrollbar
    pub const SCROLLBAR_ACTIVE: Color = Color::Yellow;
    pub const SCROLLBAR_INACTIVE: Color = Color::DarkGray;

    // Result pane selection
    pub const RESULT_ROW_ACTIVE_BG: Color = Color::Rgb(0x2e, 0x2e, 0x44);
    pub const RESULT_CELL_ACTIVE_BG: Color = Color::Rgb(0x3a, 0x3a, 0x5a);

    // Cell edit mode
    pub const CELL_EDIT_FG: Color = Color::Yellow;
    pub const CELL_DRAFT_PENDING_FG: Color = Color::Rgb(0xff, 0x99, 0x00);

    // Staged-for-delete rows
    pub const STAGED_DELETE_BG: Color = Color::Rgb(0x3d, 0x22, 0x22);
    pub const STAGED_DELETE_FG: Color = Color::Rgb(0xee, 0x77, 0x77);

    // Yank flash
    pub const YANK_FLASH_BG: Color = Color::Rgb(0xF4, 0x9E, 0x4C);
    pub const YANK_FLASH_FG: Color = Color::Rgb(0x11, 0x14, 0x19);

    // SQL syntax highlighting
    pub const SQL_KEYWORD: Color = Color::Rgb(0x89, 0xb4, 0xfa);
    pub const SQL_STRING: Color = Color::Rgb(0xa6, 0xe3, 0xa1);
    pub const SQL_NUMBER: Color = Color::Rgb(0xfa, 0xb3, 0x87);
    pub const SQL_COMMENT: Color = Color::Rgb(0x6c, 0x70, 0x86);
    pub const SQL_OPERATOR: Color = Color::Rgb(0x94, 0xe2, 0xd5);
    pub const SQL_TEXT: Color = Color::Rgb(0xcd, 0xd6, 0xf4);

    // Striped table rows — ANSI 256 (#303030) for terminal-agnostic rendering
    pub const STRIPED_ROW_BG: Color = Color::Indexed(235);

    // Text selection / cursor background in editors
    pub const SELECTION_BG: Color = Color::Black;

    // Inspector tab states
    pub const TAB_ACTIVE: Color = Color::Cyan;
    pub const TAB_INACTIVE: Color = Color::DarkGray;

    // Active/inactive toggle indicators
    pub const ACTIVE_INDICATOR: Color = Color::Green;
    pub const INACTIVE_INDICATOR: Color = Color::DarkGray;

    // Placeholder / empty-value text
    pub const PLACEHOLDER_TEXT: Color = Color::DarkGray;

    pub fn risk_color(level: RiskLevel) -> Color {
        match level {
            RiskLevel::Low => Self::STATUS_WARNING,
            RiskLevel::Medium => Self::STATUS_MEDIUM_RISK,
            RiskLevel::High => Self::STATUS_ERROR,
        }
    }
}
