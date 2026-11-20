// =============================================================================
// Cell Edit
// =============================================================================

pub mod cell_edit {
    use crate::update::action::Action;
    use crate::update::input::keybindings::KeyBinding;

    pub const WRITE: KeyBinding = KeyBinding {
        key_short: ":w",
        key: ":w",
        desc_short: "Write",
        description: "Preview and confirm UPDATE",
        action: Action::SubmitCellEditWrite,
        combos: &[], // :w is a command sequence, not a single combo
    };
}
