use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::app::ports::outbound::{
    ErDiagramExporter, ErExportResult, GraphvizError, GraphvizRunner, ViewerError, ViewerLauncher,
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
    fn open_file(&self, path: &Path, browser: Option<&str>) -> Result<(), ViewerError> {
        if let Some(browser) = browser.map(str::trim).filter(|value| !value.is_empty()) {
            open_with_browser(path, browser)?;
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            open_with_default_browser(path)?;
        }
        #[cfg(any(target_os = "freebsd", target_os = "linux"))]
        {
            Command::new("xdg-open").arg(path).spawn()?;
        }
        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/C", "start"])
                .arg(path)
                .spawn()?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn open_with_default_browser(path: &Path) -> Result<(), ViewerError> {
    if let Some(bundle_id) = default_web_browser_bundle_id() {
        Command::new("open")
            .arg("-b")
            .arg(bundle_id)
            .arg(path)
            .spawn()?;
    } else {
        Command::new("open").arg(path).spawn()?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn default_web_browser_bundle_id() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let launch_services = PathBuf::from(home)
        .join("Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist");
    let output = Command::new("plutil")
        .args(["-extract", "LSHandlers", "json", "-o", "-"])
        .arg(launch_services)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json = String::from_utf8(output.stdout).ok()?;
    default_web_browser_bundle_id_from_ls_handlers_json(&json)
}

#[cfg(any(target_os = "macos", test))]
fn default_web_browser_bundle_id_from_ls_handlers_json(json: &str) -> Option<String> {
    let handlers: serde_json::Value = serde_json::from_str(json).ok()?;
    let handlers = handlers.as_array()?;
    ["https", "http"]
        .into_iter()
        .find_map(|scheme| {
            handlers
                .iter()
                .find(|handler| {
                    handler
                        .get("LSHandlerURLScheme")
                        .and_then(serde_json::Value::as_str)
                        == Some(scheme)
                })
                .and_then(handler_bundle_id)
        })
        .or_else(|| {
            handlers
                .iter()
                .find(|handler| {
                    handler
                        .get("LSHandlerContentType")
                        .and_then(serde_json::Value::as_str)
                        == Some("com.apple.default-app.web-browser")
                })
                .and_then(handler_bundle_id)
        })
}

#[cfg(any(target_os = "macos", test))]
fn handler_bundle_id(handler: &serde_json::Value) -> Option<String> {
    handler
        .get("LSHandlerRoleAll")
        .or_else(|| handler.get("LSHandlerRoleViewer"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn open_with_browser(path: &Path, browser: &str) -> Result<(), ViewerError> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-a")
            .arg(browser)
            .arg(path)
            .spawn()?;
        Ok(())
    }

    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    {
        open_with_browser_candidates(path, browser, &browser_command_candidates(browser))
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", browser])
            .arg(path)
            .spawn()?;
        Ok(())
    }
}

#[cfg(any(target_os = "freebsd", target_os = "linux", test))]
fn browser_command_candidates(browser: &str) -> Vec<&str> {
    match browser {
        "Google Chrome" => vec![
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "chrome",
        ],
        "Firefox" => vec!["firefox"],
        "Safari" => vec![],
        "Microsoft Edge" => vec!["microsoft-edge", "microsoft-edge-stable"],
        "Brave" => vec!["brave-browser", "brave"],
        _ => vec![browser],
    }
}

#[cfg(any(target_os = "freebsd", target_os = "linux"))]
fn open_with_browser_candidates(
    path: &Path,
    browser: &str,
    candidates: &[&str],
) -> Result<(), ViewerError> {
    if candidates.is_empty() {
        return Err(ViewerError::UnsupportedBrowser {
            browser: browser.to_string(),
        });
    }

    for command in candidates {
        match Command::new(command).arg(path).spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(ViewerError::BrowserCommandNotFound {
        browser: browser.to_string(),
        candidates: candidates.join(", "),
    })
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

            let _ = writeln!(
                dot,
                "    \"{full_name}\" [label=\"{table_name}\\n({schema_name})\" style=filled fillcolor=lightblue];"
            );
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

            let _ = writeln!(
                dot,
                "    \"{from_escaped}\" -> \"{to_escaped}\" [label=\"{label_escaped}\"];"
            );
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
        browser: Option<&str>,
    ) -> ErExportResult<PathBuf> {
        let dot_path = cache_dir.join(filename);
        std::fs::write(&dot_path, dot_content)?;

        let svg_path = dot_path.with_extension("svg");
        self.graphviz.convert_dot_to_svg(&dot_path, &svg_path)?;
        self.viewer.open_file(&svg_path, browser)?;

        Self::cleanup_er_files(cache_dir, &[&dot_path, &svg_path]);

        Ok(svg_path)
    }

    fn cleanup_er_files(cache_dir: &Path, skip: &[&Path]) {
        let Ok(entries) = std::fs::read_dir(cache_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if skip.iter().any(|s| **s == path) {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.starts_with("er_")
                && Path::new(name).extension().is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("dot") || ext.eq_ignore_ascii_case("svg")
                })
            {
                let _ = std::fs::remove_file(&path);
            }
        }
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
        browser: Option<&str>,
    ) -> ErExportResult<PathBuf> {
        let dot_content = Self::generate_full_dot(tables);
        self.export(&dot_content, filename, cache_dir, browser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::er::ErFkInfo;

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
            browser: std::sync::Mutex<Option<String>>,
        }

        impl MockViewer {
            fn new() -> Self {
                Self {
                    called: AtomicBool::new(false),
                    should_fail: false,
                    browser: std::sync::Mutex::new(None),
                }
            }

            fn failing() -> Self {
                Self {
                    called: AtomicBool::new(false),
                    should_fail: true,
                    browser: std::sync::Mutex::new(None),
                }
            }
        }

        impl ViewerLauncher for MockViewer {
            fn open_file(&self, _path: &Path, browser: Option<&str>) -> Result<(), ViewerError> {
                self.called.store(true, Ordering::SeqCst);
                *self.browser.lock().unwrap() = browser.map(str::to_string);
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

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path(), None);

            assert!(result.is_ok());
            assert!(exporter.graphviz.called.load(Ordering::SeqCst));
            assert!(exporter.viewer.called.load(Ordering::SeqCst));
        }

        #[test]
        fn passes_browser_to_viewer() {
            let graphviz = MockGraphviz::new();
            let viewer = MockViewer::new();
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result =
                exporter.export("digraph {}", "test.dot", temp_dir.path(), Some("Firefox"));

            assert!(result.is_ok());
            assert_eq!(
                exporter.viewer.browser.lock().unwrap().as_deref(),
                Some("Firefox")
            );
        }

        #[test]
        fn browser_command_candidates_include_common_presets() {
            assert_eq!(
                browser_command_candidates("Microsoft Edge"),
                vec!["microsoft-edge", "microsoft-edge-stable"]
            );
            assert_eq!(
                browser_command_candidates("Brave"),
                vec!["brave-browser", "brave"]
            );
            assert_eq!(
                browser_command_candidates("CustomBrowser"),
                vec!["CustomBrowser"]
            );
        }

        #[cfg(any(target_os = "freebsd", target_os = "linux"))]
        #[test]
        fn empty_browser_candidates_return_unsupported_browser() {
            let result = open_with_browser_candidates(Path::new("/tmp/er.svg"), "Safari", &[]);

            assert!(matches!(
                result,
                Err(ViewerError::UnsupportedBrowser { browser }) if browser == "Safari"
            ));
        }

        #[test]
        fn default_browser_bundle_prefers_https_handler() {
            let json = r#"[
                {
                    "LSHandlerContentType": "com.apple.default-app.web-browser",
                    "LSHandlerRoleAll": "com.example.contenttype"
                },
                {
                    "LSHandlerURLScheme": "http",
                    "LSHandlerRoleAll": "com.example.http"
                },
                {
                    "LSHandlerURLScheme": "https",
                    "LSHandlerRoleAll": "company.thebrowser.browser"
                }
            ]"#;

            assert_eq!(
                default_web_browser_bundle_id_from_ls_handlers_json(json).as_deref(),
                Some("company.thebrowser.browser")
            );
        }

        #[test]
        fn default_browser_bundle_falls_back_to_default_browser_content_type() {
            let json = r#"[
                {
                    "LSHandlerContentType": "com.apple.default-app.web-browser",
                    "LSHandlerRoleAll": "company.thebrowser.browser"
                }
            ]"#;

            assert_eq!(
                default_web_browser_bundle_id_from_ls_handlers_json(json).as_deref(),
                Some("company.thebrowser.browser")
            );
        }

        #[test]
        fn graphviz_not_installed_returns_error() {
            let graphviz = MockGraphviz::not_installed();
            let viewer = MockViewer::new();
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path(), None);

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

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path(), None);

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

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path(), None);

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("mock failure"));
            assert!(exporter.graphviz.called.load(Ordering::SeqCst));
        }

        #[test]
        fn old_er_files_are_removed_on_export() {
            let temp_dir = tempfile::tempdir().unwrap();
            let old_dot = temp_dir.path().join("er_old_tables.dot");
            let old_svg = temp_dir.path().join("er_old_tables.svg");
            std::fs::write(&old_dot, "old dot").unwrap();
            std::fs::write(&old_svg, "old svg").unwrap();

            let exporter = DotExporter::with_dependencies(MockGraphviz::new(), MockViewer::new());
            exporter
                .export("digraph {}", "er_new.dot", temp_dir.path(), None)
                .unwrap();

            assert!(!old_dot.exists());
            assert!(!old_svg.exists());
            assert!(temp_dir.path().join("er_new.dot").exists());
        }

        #[test]
        fn non_er_files_survive_cleanup() {
            let temp_dir = tempfile::tempdir().unwrap();
            let log_file = temp_dir.path().join("er_failure.log");
            let other_file = temp_dir.path().join("other.txt");
            std::fs::write(&log_file, "log").unwrap();
            std::fs::write(&other_file, "data").unwrap();

            let exporter = DotExporter::with_dependencies(MockGraphviz::new(), MockViewer::new());
            exporter
                .export("digraph {}", "er_new.dot", temp_dir.path(), None)
                .unwrap();

            assert!(log_file.exists());
            assert!(other_file.exists());
        }

        #[test]
        fn graphviz_failure_preserves_old_files() {
            let temp_dir = tempfile::tempdir().unwrap();
            let old_dot = temp_dir.path().join("er_old_tables.dot");
            let old_svg = temp_dir.path().join("er_old_tables.svg");
            std::fs::write(&old_dot, "old dot").unwrap();
            std::fs::write(&old_svg, "old svg").unwrap();

            let exporter =
                DotExporter::with_dependencies(MockGraphviz::not_installed(), MockViewer::new());
            let result = exporter.export("digraph {}", "er_new.dot", temp_dir.path(), None);

            assert!(result.is_err());
            assert!(old_dot.exists());
            assert!(old_svg.exists());
        }

        #[test]
        fn viewer_failure_preserves_old_and_new_files() {
            let temp_dir = tempfile::tempdir().unwrap();
            let old_dot = temp_dir.path().join("er_old_tables.dot");
            let old_svg = temp_dir.path().join("er_old_tables.svg");
            std::fs::write(&old_dot, "old dot").unwrap();
            std::fs::write(&old_svg, "old svg").unwrap();

            let exporter =
                DotExporter::with_dependencies(MockGraphviz::new(), MockViewer::failing());
            let result = exporter.export("digraph {}", "er_new.dot", temp_dir.path(), None);

            assert!(result.is_err());
            assert!(temp_dir.path().join("er_new.dot").exists());
            assert!(old_dot.exists());
            assert!(old_svg.exists());
        }
    }
}
