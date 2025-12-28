pub mod clipboard;
pub mod metadata;

pub use clipboard::{ClipboardError, ClipboardWriter};
pub use metadata::{MetadataError, MetadataProvider};
