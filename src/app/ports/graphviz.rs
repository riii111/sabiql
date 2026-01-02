use std::error::Error;
use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub enum GraphvizError {
    NotInstalled,
    CommandFailed(Option<i32>),
    IoError(std::io::Error),
}

impl fmt::Display for GraphvizError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphvizError::NotInstalled => write!(
                f,
                "Graphviz (dot) not found. Please install Graphviz (e.g., brew install graphviz on macOS)"
            ),
            GraphvizError::CommandFailed(code) => {
                write!(
                    f,
                    "Graphviz failed (exit code {:?}). Check DOT syntax.",
                    code
                )
            }
            GraphvizError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl Error for GraphvizError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            GraphvizError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for GraphvizError {
    fn from(e: std::io::Error) -> Self {
        GraphvizError::IoError(e)
    }
}

#[derive(Debug)]
pub enum ViewerError {
    LaunchFailed(std::io::Error),
}

impl fmt::Display for ViewerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViewerError::LaunchFailed(e) => write!(f, "Failed to open viewer: {}", e),
        }
    }
}

impl Error for ViewerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ViewerError::LaunchFailed(e) => Some(e),
        }
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

            let message = format!("{}", error);

            assert!(message.contains("brew install graphviz"));
        }

        #[test]
        fn command_failed_contains_exit_code() {
            let error = GraphvizError::CommandFailed(Some(1));

            let message = format!("{}", error);

            assert!(message.contains("exit code"));
        }
    }

    mod viewer_error {
        use super::*;

        #[test]
        fn launch_failed_contains_cause() {
            let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "command not found");
            let error = ViewerError::LaunchFailed(io_error);

            let message = format!("{}", error);

            assert!(message.contains("command not found"));
        }
    }
}
