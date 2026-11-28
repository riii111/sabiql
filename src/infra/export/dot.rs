use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::app::ports::outbound::{ErDiagramExporter, ErExportError, ErExportResult};
use crate::config::get_cache_dir;
use crate::domain::ErTableInfo;
use crate::export::graphviz::{GraphvizError, GraphvizRunner, ViewerError, ViewerLauncher};

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
        open_with_default_viewer(path)
    }
}

#[cfg(target_os = "macos")]
fn open_with_default_viewer(path: &Path) -> Result<(), ViewerError> {
    open_with_default_browser(path)
}

#[cfg(any(target_os = "freebsd", target_os = "linux"))]
fn open_with_default_viewer(path: &Path) -> Result<(), ViewerError> {
    // gio reads the desktop entry and passes the path as a separate argument; it never
    // consults the SVG file association or invokes a shell with user-controlled text.
    let mut data_dirs = Vec::new();
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        data_dirs.push(PathBuf::from(path));
    } else if let Some(home) = std::env::var_os("HOME") {
        data_dirs.push(PathBuf::from(home).join(".local/share"));
    }
    let system_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    data_dirs.extend(std::env::split_paths(&system_dirs));
    launch_linux_browser(
        path,
        std::ffi::OsStr::new("xdg-settings"),
        std::ffi::OsStr::new("gio"),
        &data_dirs,
    )
}

#[cfg(any(target_os = "freebsd", target_os = "linux"))]
fn launch_linux_browser(
    path: &Path,
    settings_program: &std::ffi::OsStr,
    gio: &std::ffi::OsStr,
    data_dirs: &[PathBuf],
) -> Result<(), ViewerError> {
    let output = Command::new(settings_program)
        .args(["get", "default-web-browser"])
        .output()?;
    if !output.status.success() {
        return Err(ViewerError::DefaultBrowserUnavailable);
    }
    let desktop_id =
        String::from_utf8(output.stdout).map_err(|_| ViewerError::DefaultBrowserUnavailable)?;
    let desktop_id = desktop_id.trim();
    if desktop_id.is_empty()
        || desktop_id.contains(['/', '\\'])
        || !desktop_id.ends_with(".desktop")
    {
        return Err(ViewerError::DefaultBrowserUnavailable);
    }
    let desktop = data_dirs
        .iter()
        .find_map(|dir| find_desktop_entry(&dir.join("applications"), desktop_id))
        .ok_or(ViewerError::DefaultBrowserUnavailable)?;
    // Browser children can inherit these descriptors. Do not use output() pipes:
    // they would keep the ER task waiting until the browser itself exits.
    let status = Command::new(gio)
        .arg("launch")
        .arg(desktop)
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        return Err(ViewerError::BrowserLaunchFailed(format!(
            "exit status {status}"
        )));
    }
    Ok(())
}

// Desktop file IDs replace each directory separator below applications/ with '-'.
#[cfg(any(target_os = "freebsd", target_os = "linux"))]
fn find_desktop_entry(root: &Path, desktop_id: &str) -> Option<PathBuf> {
    fn search(root: &Path, dir: &Path, desktop_id: &str) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if let Some(found) = search(root, &path, desktop_id) {
                    return Some(found);
                }
            } else if kind.is_file() || kind.is_symlink() {
                let Some(relative) = path.strip_prefix(root).ok().and_then(Path::to_str) else {
                    continue;
                };
                let id = relative.replace('/', "-");
                if id == desktop_id && path.is_file() {
                    return Some(path);
                }
            }
        }
        None
    }
    search(root, root, desktop_id)
}

#[cfg(target_os = "windows")]
fn open_with_default_viewer(path: &Path) -> Result<(), ViewerError> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Shell::{ASSOCF_NONE, ASSOCSTR_EXECUTABLE, AssocQueryStringW};

    let scheme: Vec<u16> = "https\0".encode_utf16().collect();
    let mut length = 0u32;
    // The first call asks Windows for the buffer size of the current HTTPS handler.
    unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            scheme.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut length,
        )
    };
    if length == 0 {
        return Err(ViewerError::DefaultBrowserUnavailable);
    }
    let mut executable = vec![0u16; length as usize];
    let status = unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            scheme.as_ptr(),
            std::ptr::null(),
            executable.as_mut_ptr(),
            &mut length,
        )
    };
    if status < 0 || length <= 1 {
        return Err(ViewerError::DefaultBrowserUnavailable);
    }
    executable.truncate(length as usize - 1);
    Command::new(std::ffi::OsString::from_wide(&executable))
        .arg(path)
        .spawn()?;
    Ok(())
}

#[cfg(not(any(
    target_os = "freebsd",
    target_os = "macos",
    target_os = "linux",
    target_os = "windows"
)))]
fn open_with_default_viewer(_path: &Path) -> Result<(), ViewerError> {
    Err(ViewerError::UnsupportedPlatform {
        operation: "Opening ER diagrams".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn open_with_default_browser(path: &Path) -> Result<(), ViewerError> {
    if let Some(bundle_id) = default_web_browser_bundle_id() {
        return spawn_viewer("open", &["-b", &bundle_id], path);
    }
    Err(ViewerError::DefaultBrowserUnavailable)
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

#[cfg(target_os = "macos")]
fn spawn_viewer(program: &str, args: &[&str], path: &Path) -> Result<(), ViewerError> {
    Command::new(program).args(args).arg(path).spawn()?;
    Ok(())
}

pub struct DotExporter {
    graphviz: Box<dyn GraphvizRunner>,
    viewer: Box<dyn ViewerLauncher>,
}

impl Default for DotExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl DotExporter {
    pub fn new() -> Self {
        Self {
            graphviz: Box::new(SystemGraphvizRunner),
            viewer: Box::new(SystemViewerLauncher),
        }
    }
}

impl DotExporter {
    fn escape_dot_string(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }

    fn generate_full_dot(tables: &[ErTableInfo]) -> String {
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

impl DotExporter {
    fn export(
        &self,
        dot_content: &str,
        filename: &str,
        cache_dir: &Path,
    ) -> ErExportResult<PathBuf> {
        let dot_path = cache_dir.join(filename);
        std::fs::write(&dot_path, dot_content)?;

        let svg_path = dot_path.with_extension("svg");
        self.graphviz.convert_dot_to_svg(&dot_path, &svg_path)?;
        self.viewer.open_file(&svg_path)?;

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

impl ErDiagramExporter for DotExporter {
    fn generate_and_export(
        &self,
        tables: &[ErTableInfo],
        filename: &str,
        project_name: &str,
    ) -> ErExportResult<PathBuf> {
        let dot_content = Self::generate_full_dot(tables);
        let cache_dir = get_cache_dir(project_name)
            .map_err(|error| ErExportError::Export(error.to_string()))?;
        self.export(&dot_content, filename, &cache_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::er::ErFkInfo;

    impl DotExporter {
        fn with_dependencies<G: GraphvizRunner + 'static, V: ViewerLauncher + 'static>(
            graphviz: G,
            viewer: V,
        ) -> Self {
            Self {
                graphviz: Box::new(graphviz),
                viewer: Box::new(viewer),
            }
        }
    }

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
                    name: r#"fk_"user""#.to_string(),
                    from_qualified: "public.orders".to_string(),
                    to_qualified: "public.users".to_string(),
                }],
            },
        ]
    }

    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    #[test]
    fn linux_uses_default_browser_desktop_entry_for_svg() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let applications = dir.path().join("applications");
        std::fs::create_dir(&applications).unwrap();
        std::fs::write(applications.join("browser.desktop"), "browser").unwrap();
        std::fs::write(applications.join("image-editor.desktop"), "editor").unwrap();
        let settings = dir.path().join("settings");
        let gio = dir.path().join("gio");
        let transcript = dir.path().join("transcript");
        std::fs::write(&settings, "#!/bin/sh\nprintf 'browser.desktop\\n'\n").unwrap();
        std::fs::write(
            &gio,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
                transcript.display()
            ),
        )
        .unwrap();
        for program in [&settings, &gio] {
            std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let svg = dir.path().join("diagram.svg");
        launch_linux_browser(
            &svg,
            settings.as_os_str(),
            gio.as_os_str(),
            &[dir.path().into()],
        )
        .unwrap();
        for _ in 0..100 {
            if let Ok(record) = std::fs::read_to_string(&transcript) {
                assert!(record.contains("browser.desktop"));
                assert!(!record.contains("image-editor.desktop"));
                assert!(record.contains("diagram.svg"));
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("gio was not invoked");
    }

    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    #[test]
    fn resolves_nested_desktop_entry_id() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("applications/vendor");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("browser.desktop"), "browser").unwrap();
        assert_eq!(
            find_desktop_entry(&dir.path().join("applications"), "vendor-browser.desktop"),
            Some(nested.join("browser.desktop"))
        );
    }

    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    #[test]
    fn linux_browser_launch_failure_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("applications")).unwrap();
        std::fs::write(dir.path().join("applications/browser.desktop"), "browser").unwrap();
        let settings = dir.path().join("settings");
        let gio = dir.path().join("gio");
        std::fs::write(&settings, "#!/bin/sh\nprintf 'browser.desktop\\n'\n").unwrap();
        std::fs::write(&gio, "#!/bin/sh\nprintf 'launcher failed' >&2\nexit 1\n").unwrap();
        for program in [&settings, &gio] {
            std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let error = launch_linux_browser(
            Path::new("diagram.svg"),
            settings.as_os_str(),
            gio.as_os_str(),
            &[dir.path().into()],
        )
        .unwrap_err();
        assert!(error.to_string().contains("exit status"));
    }

    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    #[test]
    fn browser_child_does_not_hold_launch_open() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("applications")).unwrap();
        std::fs::write(dir.path().join("applications/browser.desktop"), "browser").unwrap();
        let settings = dir.path().join("settings");
        let gio = dir.path().join("gio");
        std::fs::write(&settings, "#!/bin/sh\nprintf 'browser.desktop\\n'\n").unwrap();
        std::fs::write(&gio, "#!/bin/sh\nsleep 3 &\nexit 0\n").unwrap();
        for program in [&settings, &gio] {
            std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let start = Instant::now();
        launch_linux_browser(
            Path::new("diagram.svg"),
            settings.as_os_str(),
            gio.as_os_str(),
            &[dir.path().into()],
        )
        .unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    mod generate_full_dot {
        use super::*;

        #[test]
        fn tables_appear_as_nodes() {
            let tables = make_test_tables();

            let dot = DotExporter::generate_full_dot(&tables);

            assert!(dot.contains("\"public.users\""));
            assert!(dot.contains("\"public.orders\""));
        }

        #[test]
        fn foreign_keys_appear_as_edges_with_escaped_labels() {
            let tables = make_test_tables();

            let dot = DotExporter::generate_full_dot(&tables);

            assert!(dot.contains(r#""public.orders" -> "public.users" [label="fk_\"user\""]"#));
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

            let dot = DotExporter::generate_full_dot(&tables);

            let first_pos = dot.find("\"a.first\"").unwrap();
            let last_pos = dot.find("\"z.last\"").unwrap();
            assert!(first_pos < last_pos);
        }
    }

    mod export {
        use super::*;
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };

        enum GraphvizFailure {
            None,
            NotInstalled,
            CommandFailed(i32),
        }

        struct MockGraphviz {
            called: Arc<AtomicBool>,
            failure: GraphvizFailure,
        }

        impl MockGraphviz {
            fn new() -> Self {
                Self {
                    called: Arc::new(AtomicBool::new(false)),
                    failure: GraphvizFailure::None,
                }
            }

            fn not_installed() -> Self {
                Self {
                    called: Arc::new(AtomicBool::new(false)),
                    failure: GraphvizFailure::NotInstalled,
                }
            }

            fn command_failed(exit_code: i32) -> Self {
                Self {
                    called: Arc::new(AtomicBool::new(false)),
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
            called: Arc<AtomicBool>,
            should_fail: bool,
        }

        impl MockViewer {
            fn new() -> Self {
                Self {
                    called: Arc::new(AtomicBool::new(false)),
                    should_fail: false,
                }
            }

            fn failing() -> Self {
                Self {
                    called: Arc::new(AtomicBool::new(false)),
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
            let graphviz_called = Arc::clone(&graphviz.called);
            let viewer_called = Arc::clone(&viewer.called);
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path());

            assert!(result.is_ok());
            assert!(graphviz_called.load(Ordering::SeqCst));
            assert!(viewer_called.load(Ordering::SeqCst));
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
            let viewer_called = Arc::clone(&viewer.called);
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path());

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("Graphviz"));
            assert!(!viewer_called.load(Ordering::SeqCst));
        }

        #[test]
        fn graphviz_command_failed_includes_exit_code() {
            let graphviz = MockGraphviz::command_failed(1);
            let viewer = MockViewer::new();
            let viewer_called = Arc::clone(&viewer.called);
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path());

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("Graphviz failed"));
            assert!(err_msg.contains("exit code"));
            assert!(!viewer_called.load(Ordering::SeqCst));
        }

        #[test]
        fn viewer_failure_returns_error() {
            let graphviz = MockGraphviz::new();
            let viewer = MockViewer::failing();
            let graphviz_called = Arc::clone(&graphviz.called);
            let exporter = DotExporter::with_dependencies(graphviz, viewer);
            let temp_dir = tempfile::tempdir().unwrap();

            let result = exporter.export("digraph {}", "test.dot", temp_dir.path());

            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("mock failure"));
            assert!(graphviz_called.load(Ordering::SeqCst));
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
                .export("digraph {}", "er_new.dot", temp_dir.path())
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
                .export("digraph {}", "er_new.dot", temp_dir.path())
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
            let result = exporter.export("digraph {}", "er_new.dot", temp_dir.path());

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
            let result = exporter.export("digraph {}", "er_new.dot", temp_dir.path());

            assert!(result.is_err());
            assert!(temp_dir.path().join("er_new.dot").exists());
            assert!(old_dot.exists());
            assert!(old_svg.exists());
        }
    }
}
