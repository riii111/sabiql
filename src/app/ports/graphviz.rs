use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum GraphvizError {
    #[error(
        "Graphviz (dot) not found. Please install Graphviz (e.g., brew install graphviz on macOS)"
    )]
    NotInstalled,
    #[error("Graphviz failed (exit code {0:?}). Check DOT syntax.")]
    CommandFailed(Option<i32>),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error("Failed to open viewer: {0}")]
    LaunchFailed(#[source] std::io::Error),
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

            assert!(message.contains("brew install graphviz"));
        }

        #[test]
        fn command_failed_contains_exit_code() {
            let error = GraphvizError::CommandFailed(Some(1));

            let message = format!("{error}");

            assert!(message.contains("exit code"));
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
    }
}
