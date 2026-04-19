//! Input responsibilities are split by role:
//! - `vim`: shared vim-style semantics and action resolution
//! - `keybindings`: surface-specific key handling
//! - `keymap`: low-level key translation
//! - `command`: command-line input flow

pub mod command;
pub mod keybindings;
pub mod keymap;
pub mod vim;
