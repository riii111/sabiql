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

    // Scrim (dimmed background behind modals)
    pub const SCRIM_BG: Color = Color::Rgb(0x10, 0x10, 0x18);

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
}
