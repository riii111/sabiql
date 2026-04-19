//! Input responsibilities are split by role:
//! - `vim`: shared vim-style semantics and action resolution
//! - `keybindings`: surface-specific key handling
//! - `keymap`: low-level key translation
//! - `command`: command-line input flow
//! - `dispatch`: app-owned input interpretation behind the inbound input port

pub mod command;
pub mod dispatch;
pub mod keybindings;
pub mod keymap;
pub mod vim;
