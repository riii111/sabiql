use std::path::{Path, PathBuf};
use std::process::Command;

use crate::app::ports::{
    ErDiagramExporter, GraphvizError, GraphvizRunner, ViewerError, ViewerLauncher,
};
use crate::domain::ErTableInfo;

pub struct SystemGraphvizRunner;

impl GraphvizRunner for SystemGraphvizRunner {
    fn convert_dot_to_svg(&self, dot_path: &Path, svg_path: &Path) -> Result<(), GraphvizError> {
        let status = Command::new("dot")
            .args(["-Tsvg", "-o"])
            .arg(svg_path)
            .arg(dot_path)
            .status()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GraphvizError::NotInstalled
                } else {
                    GraphvizError::IoError(e)
                }
            })?;

        if !status.success() {
            return Err(GraphvizError::CommandFailed(status.code()));
        }

        Ok(())
    }
}

pub struct SystemViewerLauncher;

impl ViewerLauncher for SystemViewerLauncher {
    fn open_file(&self, path: &Path) -> Result<(), ViewerError> {
        if let Ok(browser) = std::env::var("SABIQL_BROWSER") {
            #[cfg(target_os = "macos")]
            {
                Command::new("open")
                    .arg("-a")
                    .arg(&browser)
                    .arg(path)
                    .spawn()
                    .map_err(ViewerError::LaunchFailed)?;
            }
            #[cfg(not(target_os = "macos"))]
            {
                Command::new(&browser)
                    .arg(path)
                    .spawn()
                    .map_err(ViewerError::LaunchFailed)?;
            }
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .arg(path)
                .spawn()
                .map_err(ViewerError::LaunchFailed)?;
        }
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open")
                .arg(path)
                .spawn()
                .map_err(ViewerError::LaunchFailed)?;
        }
        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/C", "start"])
                .arg(path)
                .spawn()
                .map_err(ViewerError::LaunchFailed)?;
        }
        Ok(())
    }
}

pub struct DotExporter<G = SystemGraphvizRunner, V = SystemViewerLauncher> {
    graphviz: G,
    viewer: V,
}

impl Default for DotExporter<SystemGraphvizRunner, SystemViewerLauncher> {
    fn default() -> Self {
        Self::new()
    }
}

impl DotExporter<SystemGraphvizRunner, SystemViewerLauncher> {
    pub fn new() -> Self {
        Self {
            graphviz: SystemGraphvizRunner,
            viewer: SystemViewerLauncher,
        }
    }
}

#[cfg(test)]
impl<G: GraphvizRunner, V: ViewerLauncher> DotExporter<G, V> {
    pub fn with_dependencies(graphviz: G, viewer: V) -> Self {
        Self { graphviz, viewer }
    }
}

impl<G, V> DotExporter<G, V> {
    fn escape_dot_string(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }

    pub fn generate_full_dot(tables: &[ErTableInfo]) -> String {
        let mut dot = String::new();
        dot.push_str("digraph full_er {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=box, fontname=\"Helvetica\"];\n");
        dot.push_str("    edge [fontname=\"Helvetica\", fontsize=10];\n");
        dot.push('\n');

        let mut sorted_tables: Vec<_> = tables.iter().collect();
        sorted_tables.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

        for table in &sorted_tables {
            let full_name = Self::escape_dot_string(&table.qualified_name);
            let table_name = Self::escape_dot_string(&table.name);
            let schema_name = Self::escape_dot_string(&table.schema);

            dot.push_str(&format!(
                "    \"{}\" [label=\"{}\\n({})\" style=filled fillcolor=lightblue];\n",
                full_name, table_name, schema_name
            ));
        }

        dot.push('\n');

        let mut edges: Vec<_> = sorted_tables
            .iter()
            .flat_map(|table| {
                table.foreign_keys.iter().map(|fk| {
                    (
                        fk.from_qualified.clone(),
                        fk.to_qualified.clone(),
                        fk.name.clone(),
                    )
                })
            })
            .collect();
        edges.sort();

        for (from, to, label) in edges {
            let from_escaped = Self::escape_dot_string(&from);
            let to_escaped = Self::escape_dot_string(&to);
            let label_escaped = Self::escape_dot_string(&label);

            dot.push_str(&format!(
                "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
                from_escaped, to_escaped, label_escaped
            ));
        }

        dot.push_str("}\n");
        dot
    }
}

impl<G: GraphvizRunner, V: ViewerLauncher> DotExporter<G, V> {
    pub fn export(
        &self,
        dot_content: &str,
        filename: &str,
        cache_dir: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let dot_path = cache_dir.join(filename);
        std::fs::write(&dot_path, dot_content)?;

        let svg_path = dot_path.with_extension("svg");
        self.graphviz.convert_dot_to_svg(&dot_path, &svg_path)?;
        self.viewer.open_file(&svg_path)?;

        Ok(svg_path)
    }
}

impl<G: GraphvizRunner + 'static, V: ViewerLauncher + 'static> ErDiagramExporter
    for DotExporter<G, V>
{
    fn generate_and_export(
        &self,
        tables: &[ErTableInfo],
        filename: &str,
        cache_dir: &Path,
    ) -> crate::app::ports::ErExportResult<PathBuf> {
        let dot_content = Self::generate_full_dot(tables);
        self.export(&dot_content, filename, cache_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ErFkInfo;

    fn make_test_tables() -> Vec<ErTableInfo> {
        vec![
            ErTableInfo {
                qualified_name: "public.users".to_string(),
                name: "users".to_string(),
                schema: "public".to_string(),
                foreign_keys: vec![],
            },
            ErTableInfo {
                qualified_name: "public.orders".to_string(),
                name: "orders".to_string(),
                schema: "public".to_string(),
                foreign_keys: vec![ErFkInfo {
                    name: "fk_user".to_string(),
                    from_qualified: "public.orders".to_string(),
                    to_qualified: "public.users".to_string(),
                }],
            },
        ]
    }

    mod generate_full_dot {
        use super::*;

        #[test]
        fn tables_appear_as_nodes() {
            let tables = make_test_tables();

            let dot = DotExporter::<SystemGraphvizRunner, SystemViewerLauncher>::generate_full_dot(
                &tables,
            );

            assert!(dot.contains("\"public.users\""));
            assert!(dot.contains("\"public.orders\""));
        }

        #[test]
        fn foreign_keys_appear_as_edges() {
            let tables = make_test_tables();

            let dot = DotExporter::<SystemGraphvizRunner, SystemViewerLauncher>::generate_full_dot(
                &tables,
            );

            assert!(dot.contains("\"public.orders\" -> \"public.users\""));
            assert!(dot.contains("label=\"fk_user\""));
        }

        #[test]
        fn output_is_sorted_for_stability() {
            let tables = vec![
                ErTableInfo {
                    qualified_name: "z.last".to_string(),
                    name: "last".to_string(),
                    schema: "z".to_string(),
                    foreign_keys: vec![],
                },
                ErTableInfo {
                    qualified_name: "a.first".to_string(),
                    name: "first".to_string(),
                    schema: "a".to_string(),
                    foreign_keys: vec![],
                },
            ];

            let dot = DotExporter::<SystemGraphvizRunner, SystemViewerLauncher>::generate_full_dot(
                &tables,
            );

            let first_pos = dot.find("\"a.first\"").unwrap();
            let last_pos = dot.find("\"z.last\"").unwrap();
            assert!(first_pos < last_pos);
        }
    }

    mod export {
        use super::*;
        use std::sync::atomic::{AtomicBool, Ordering};

        enum GraphvizFailure {
            None,
            NotInstalled,
            CommandFailed(i32),
        }

        struct MockGraphviz {
            called: AtomicBool,
            failure: GraphvizFailure,
        }

        impl MockGraphviz {
            fn new() -> Self {
                Self {
                    called: AtomicBool::new(false),
                    failure: GraphvizFailure::None,
                }
            }

            fn not_installed() -> Self {
                Self {
                    called: AtomicBool::new(false),
                    failure: GraphvizFailure::NotInstalled,
                }
            }

            fn command_failed(exit_code: i32) -> Self {
                Self {
                    called: AtomicBool::new(false),
                    failure: GraphvizFailure::CommandFailed(exit_code),
                }
            }
        }

        impl GraphvizRunner for MockGraphviz {
            fn convert_dot_to_svg(
                &self,
                _dot_path: &Path,
                _svg_path: &Path,
            ) -> Result<(), GraphvizError> {
                self.called.store(true, Ordering::SeqCst);
                match &self.failure {
                    GraphvizFailure::None => Ok(()),
                    GraphvizFailure::NotInstalled => Err(GraphvizError::NotInstalled),
                    GraphvizFailure::CommandFailed(code) => {
                        Err(GraphvizError::CommandFailed(Some(*code)))
                    }
                }
            }
        }

        struct MockViewer {
            called: AtomicBool,
            should_fail: bool,
        }

        impl MockViewer {
            fn new() -> Self {
                Self {
                    called: AtomicBool::new(false),
                    should_fail: false,
                }
            }

            fn failing() -> Self {
                Self {
                    called: AtomicBool::new(false),
                    should_fail: true,
                }
            }
        }

        impl ViewerLauncher for MockViewer {
            fn open_file(&self, _path: &Path) -> Result<(), ViewerError> {
                self.called.store(true, Ordering::SeqCst);
                if self.should_fail {
                    Err(ViewerError::LaunchFailed(std::io::Error::other(
                        "mock failure",
                    )))
                } else {
                    Ok(())
                }
            }
        }

        #[test]
        fn calls_graphviz_and_viewer() {
            let graphviz = MockGraphviz::new();
            let viewer = MockViewer::new();
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path());

            assert!(result.is_ok());
            assert!(exporter.graphviz.called.load(Ordering::SeqCst));
            assert!(exporter.viewer.called.load(Ordering::SeqCst));
        }

        #[test]
        fn graphviz_not_installed_returns_error() {
            let graphviz = MockGraphviz::not_installed();
            let viewer = MockViewer::new();
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path());

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("Graphviz"));
            assert!(!exporter.viewer.called.load(Ordering::SeqCst));
        }

        #[test]
        fn graphviz_command_failed_includes_exit_code() {
            let graphviz = MockGraphviz::command_failed(1);
            let viewer = MockViewer::new();
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path());

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("Graphviz failed"));
            assert!(err_msg.contains("exit code"));
            assert!(!exporter.viewer.called.load(Ordering::SeqCst));
        }

        #[test]
        fn viewer_failure_returns_error() {
            let graphviz = MockGraphviz::new();
            let viewer = MockViewer::failing();
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path());

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("mock failure"));
            assert!(exporter.graphviz.called.load(Ordering::SeqCst));
        }
    }
}
