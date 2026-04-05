use ratatui::style::Color;

use crate::app::policy::write::write_guardrails::RiskLevel;

pub struct Theme;

impl Theme {
    // Modal border
    pub const MODAL_BORDER: Color = Color::Rgb(0x45, 0x47, 0x55);
    pub const MODAL_BORDER_HIGHLIGHT: Color = Color::Rgb(0xb0, 0xb4, 0xbe);

    // Modal title (emphasized)
    pub const MODAL_TITLE: Color = Color::Rgb(0xc9, 0xce, 0xd8);

    // Modal hint text (de-emphasized)
    pub const MODAL_HINT: Color = Color::Rgb(0x5b, 0x5f, 0x6e);

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
    pub const FOCUS_BORDER: Color = Color::Rgb(0x97, 0xc9, 0xc3);
    pub const UNFOCUS_BORDER: Color = Color::Rgb(0x45, 0x47, 0x55);
    pub const HIGHLIGHT_BORDER: Color = Color::Rgb(0xb0, 0xdd, 0xd8);

    // ============ Text Colors ============

    // Semantic text colors
    pub const TEXT_PRIMARY: Color = Color::Rgb(0xc9, 0xce, 0xd8);
    pub const TEXT_SECONDARY: Color = Color::Rgb(0xb0, 0xb4, 0xbe);
    pub const TEXT_MUTED: Color = Color::Rgb(0x5b, 0x5f, 0x6e);
    pub const TEXT_DIM: Color = Color::Rgb(0x77, 0x77, 0x88);
    pub const TEXT_ACCENT: Color = Color::Rgb(0xc4, 0xb2, 0x8a);

    // ============ Status Colors ============

    // Status indicators
    pub const STATUS_SUCCESS: Color = Color::Rgb(0x97, 0xc9, 0xc3);
    pub const STATUS_ERROR: Color = Color::Rgb(0xc4, 0x74, 0x6e);
    pub const STATUS_WARNING: Color = Color::Rgb(0xc4, 0xb2, 0x8a);
    pub const STATUS_MEDIUM_RISK: Color = Color::Rgb(0xff, 0x99, 0x00);

    // ============ Component Colors ============

    // Cursor
    pub const CURSOR_FG: Color = Color::White;

    // Section headers
    pub const SECTION_HEADER: Color = Color::Rgb(0x97, 0xc9, 0xc3);

    // Scrollbar
    pub const SCROLLBAR_ACTIVE: Color = Color::Rgb(0x6a, 0x9e, 0x98);
    pub const SCROLLBAR_INACTIVE: Color = Color::Rgb(0x45, 0x47, 0x55);

    // Result pane selection
    pub const RESULT_ROW_ACTIVE_BG: Color = Color::Rgb(0x2e, 0x2e, 0x44);
    pub const RESULT_CELL_ACTIVE_BG: Color = Color::Rgb(0x3a, 0x3a, 0x5a);

    // Cell edit mode
    pub const CELL_EDIT_FG: Color = Color::Rgb(0xc4, 0xb2, 0x8a);
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
    pub const SQL_TEXT: Color = Color::Rgb(0xc9, 0xce, 0xd8);

    // JSON tree highlighting (muted to match overall theme)
    pub const JSON_KEY: Color = Color::Rgb(0x97, 0xc9, 0xc3);
    pub const JSON_STRING: Color = Color::Rgb(0x8a, 0xb8, 0x8a);
    pub const JSON_NUMBER: Color = Color::Rgb(0xc8, 0x9b, 0x7a);
    pub const JSON_BOOL: Color = Color::Rgb(0xc8, 0x9b, 0x7a);
    pub const JSON_NULL: Color = Color::Rgb(0x5b, 0x5f, 0x6e);
    pub const JSON_BRACKET: Color = Color::Rgb(0xb0, 0xb4, 0xbe);

    // Striped table rows
    pub const STRIPED_ROW_BG: Color = Color::Rgb(0x1e, 0x1e, 0x23);

    // Text selection / cursor background in editors
    pub const SELECTION_BG: Color = Color::Black;

    // Inspector tab states
    pub const TAB_ACTIVE: Color = Color::Rgb(0x97, 0xc9, 0xc3);
    pub const TAB_INACTIVE: Color = Color::Rgb(0x5b, 0x5f, 0x6e);

    // Active/inactive toggle indicators
    pub const ACTIVE_INDICATOR: Color = Color::Rgb(0x97, 0xc9, 0xc3);
    pub const INACTIVE_INDICATOR: Color = Color::Rgb(0x5b, 0x5f, 0x6e);

    // Placeholder / empty-value text
    pub const PLACEHOLDER_TEXT: Color = Color::Rgb(0x5b, 0x5f, 0x6e);

    pub fn risk_color(level: RiskLevel) -> Color {
        match level {
            RiskLevel::Low => Self::STATUS_WARNING,
            RiskLevel::Medium => Self::STATUS_MEDIUM_RISK,
            RiskLevel::High => Self::STATUS_ERROR,
        }
    }
}
