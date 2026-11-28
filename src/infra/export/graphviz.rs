use std::path::Path;

use crate::app::ports::outbound::ErExportError;

#[derive(Debug, thiserror::Error)]
pub enum GraphvizError {
    #[error("Graphviz (dot) not found. Install Graphviz and ensure dot is available on PATH")]
    NotInstalled,
    #[error("Graphviz failed (exit code {0:?}). Check DOT syntax.")]
    CommandFailed(Option<i32>),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error("Failed to open viewer: {0}")]
    LaunchFailed(#[from] std::io::Error),
    #[error("{operation} is unsupported on this platform")]
    UnsupportedPlatform { operation: String },
}

impl From<GraphvizError> for ErExportError {
    fn from(e: GraphvizError) -> Self {
        Self::Export(e.to_string())
    }
}

impl From<ViewerError> for ErExportError {
    fn from(e: ViewerError) -> Self {
        Self::Export(e.to_string())
    }
}

pub trait GraphvizRunner: Send + Sync {
    fn convert_dot_to_svg(&self, dot_path: &Path, svg_path: &Path) -> Result<(), GraphvizError>;
}

pub trait ViewerLauncher: Send + Sync {
    fn open_file(&self, path: &Path) -> Result<(), ViewerError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    mod graphviz_error {
        use super::*;

        #[test]
        fn not_installed_contains_installation_hint() {
            let error = GraphvizError::NotInstalled;

            let message = format!("{error}");

            assert!(message.contains("Install Graphviz"));
            assert!(message.contains("PATH"));
        }
    }

    mod viewer_error {
        use super::*;

        #[test]
        fn launch_failed_contains_cause() {
            let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "command not found");
            let error = ViewerError::LaunchFailed(io_error);

            let message = format!("{error}");

            assert!(message.contains("command not found"));
        }

        #[test]
        fn unsupported_platform_names_operation() {
            let error = ViewerError::UnsupportedPlatform {
                operation: "Opening ER diagrams".to_string(),
            };

            let message = format!("{error}");

            assert_eq!(
                message,
                "Opening ER diagrams is unsupported on this platform"
            );
        }
    }
}
