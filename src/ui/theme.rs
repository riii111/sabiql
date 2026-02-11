use ratatui::style::Color;

/// Base color palette
#[allow(dead_code)]
pub struct Palette;

#[allow(dead_code)]
impl Palette {
    pub const CATPPUCCIN_MOCHA: Color = Color::Rgb(0x1e, 0x1e, 0x2e);
    pub const DUCKBONES: Color = Color::Rgb(0x15, 0x19, 0x26);
}

/// Application color theme constants
#[allow(dead_code)]
pub struct Theme;

#[allow(dead_code)]
impl Theme {
    // Modal/Overlay backgrounds
    pub const MODAL_BG: Color = Palette::DUCKBONES;

    // Modal border
    pub const MODAL_BORDER: Color = Color::Rgb(0x3a, 0x3a, 0x4a);
    pub const MODAL_BORDER_HIGHLIGHT: Color = Color::Rgb(0x5a, 0x5a, 0x7a);

    // Modal title (emphasized)
    pub const MODAL_TITLE: Color = Color::Rgb(0xcc, 0xcc, 0xdd);

    // Modal hint text (de-emphasized)
    pub const MODAL_HINT: Color = Color::Rgb(0x55, 0x55, 0x66);

    // Key chip (for important keys in Help)
    pub const KEY_CHIP_BG: Color = Color::Rgb(0x3a, 0x3a, 0x4a);
    pub const KEY_CHIP_FG: Color = Color::Rgb(0xee, 0xcc, 0x66);

    // SQL Editor current line highlight
    pub const EDITOR_CURRENT_LINE_BG: Color = Color::Rgb(0x22, 0x26, 0x33);

    // Completion popup
    pub const COMPLETION_SELECTED_BG: Color = Color::Rgb(0x45, 0x47, 0x5a);

    // Table header/alternating row backgrounds
    pub const TABLE_HEADER_BG: Color = Color::Rgb(0x2a, 0x2a, 0x2e);

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
    pub const TEXT_ACCENT: Color = Color::Yellow;

    // ============ Status Colors ============

    // Status indicators
    pub const STATUS_SUCCESS: Color = Color::Green;
    pub const STATUS_ERROR: Color = Color::Red;
    pub const STATUS_WARNING: Color = Color::Yellow;

    // ============ Component Colors ============

    // Cursor
    pub const CURSOR_FG: Color = Color::White;

    // Section headers
    pub const SECTION_HEADER: Color = Color::Cyan;

    // Scrollbar
    pub const SCROLLBAR_ACTIVE: Color = Color::Yellow;
    pub const SCROLLBAR_INACTIVE: Color = Color::DarkGray;
}
