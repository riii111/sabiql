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
    LaunchFailed(#[from] std::io::Error),
    #[error("{browser} is not available on this platform")]
    UnsupportedBrowser { browser: String },
    #[error("Browser command not found for {browser}. Tried: {candidates}")]
    BrowserCommandNotFound { browser: String, candidates: String },
}

pub trait GraphvizRunner: Send + Sync {
    fn convert_dot_to_svg(&self, dot_path: &Path, svg_path: &Path) -> Result<(), GraphvizError>;
}

pub trait ViewerLauncher: Send + Sync {
    fn open_file(&self, path: &Path, browser: Option<&str>) -> Result<(), ViewerError>;
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

        #[test]
        fn unsupported_browser_names_browser() {
            let error = ViewerError::UnsupportedBrowser {
                browser: "Safari".to_string(),
            };

            let message = format!("{error}");

            assert!(message.contains("Safari"));
            assert!(message.contains("not available"));
        }

        #[test]
        fn browser_command_not_found_lists_candidates() {
            let error = ViewerError::BrowserCommandNotFound {
                browser: "Google Chrome".to_string(),
                candidates: "google-chrome, chromium".to_string(),
            };

            let message = format!("{error}");

            assert!(message.contains("Google Chrome"));
            assert!(message.contains("google-chrome"));
            assert!(message.contains("chromium"));
        }
    }
}
