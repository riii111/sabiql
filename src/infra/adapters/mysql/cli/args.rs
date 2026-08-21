use std::path::Path;

pub(super) const MYSQL_CLIENT_MAX_PACKET_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn mysql_connection_args(option_file: &Path) -> Vec<String> {
    vec![
        format!("--defaults-file={}", option_file.display()),
        "--no-login-paths".to_string(),
        "--protocol=TCP".to_string(),
        "--connect-timeout=10".to_string(),
        "--skip-reconnect".to_string(),
    ]
}

pub(super) fn mysql_query_args(option_file: &Path) -> Vec<String> {
    mysql_result_args(option_file, true)
}

pub(super) fn mysql_adhoc_args(option_file: &Path) -> Vec<String> {
    let mut args = mysql_query_args(option_file);
    args.push("--show-warnings".to_string());
    args
}

pub(super) fn mysql_metadata_session_args(option_file: &Path) -> Vec<String> {
    mysql_result_args(option_file, false)
}

fn mysql_result_args(option_file: &Path, quick: bool) -> Vec<String> {
    let mut args = mysql_connection_args(option_file);
    args.extend([
        "--xml".to_string(),
        "--binary-as-hex".to_string(),
        "--binary-mode".to_string(),
    ]);
    if quick {
        args.extend([
            format!("--max-allowed-packet={MYSQL_CLIENT_MAX_PACKET_BYTES}"),
            "--quick".to_string(),
        ]);
    }
    args.extend([
        "--unbuffered".to_string(),
        "--default-character-set=utf8mb4".to_string(),
        "--batch".to_string(),
        "--silent".to_string(),
        "--prompt=".to_string(),
    ]);
    args
}

pub(super) fn mysql_metadata_args(option_file: &Path) -> Vec<String> {
    let mut args = mysql_connection_args(option_file);
    args.extend([
        "--batch".to_string(),
        "--column-names".to_string(),
        "--column-type-info".to_string(),
        "--binary-as-hex".to_string(),
        "--binary-mode".to_string(),
        "--unbuffered".to_string(),
        "--default-character-set=utf8mb4".to_string(),
        "--prompt=".to_string(),
    ]);
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_arguments_keep_defaults_file_first_and_disable_login_paths() {
        let args = mysql_connection_args(Path::new("/tmp/sabiql-mysql.cnf"));

        assert_eq!(
            args,
            vec![
                "--defaults-file=/tmp/sabiql-mysql.cnf",
                "--no-login-paths",
                "--protocol=TCP",
                "--connect-timeout=10",
                "--skip-reconnect",
            ]
        );
    }

    #[test]
    fn adhoc_preview_and_export_arguments_include_streaming_query_options() {
        let args = mysql_query_args(Path::new("/tmp/sabiql-mysql.cnf"));

        assert_eq!(
            args,
            vec![
                "--defaults-file=/tmp/sabiql-mysql.cnf",
                "--no-login-paths",
                "--protocol=TCP",
                "--connect-timeout=10",
                "--skip-reconnect",
                "--xml",
                "--binary-as-hex",
                "--binary-mode",
                "--max-allowed-packet=33554432",
                "--quick",
                "--unbuffered",
                "--default-character-set=utf8mb4",
                "--batch",
                "--silent",
                "--prompt=",
            ]
        );
        assert!(args.iter().all(|argument| !argument.contains("password")));
    }

    #[test]
    fn metadata_arguments_append_metadata_options_after_connection_arguments() {
        let args = mysql_metadata_args(Path::new("/tmp/sabiql-mysql.cnf"));

        assert_eq!(
            args,
            vec![
                "--defaults-file=/tmp/sabiql-mysql.cnf",
                "--no-login-paths",
                "--protocol=TCP",
                "--connect-timeout=10",
                "--skip-reconnect",
                "--batch",
                "--column-names",
                "--column-type-info",
                "--binary-as-hex",
                "--binary-mode",
                "--unbuffered",
                "--default-character-set=utf8mb4",
                "--prompt=",
            ]
        );
        assert!(!args.iter().any(|argument| argument == "--quick"));
        assert!(args.iter().all(|argument| !argument.contains("password")));
    }

    #[test]
    fn metadata_session_arguments_keep_result_options_without_quick() {
        let args = mysql_metadata_session_args(Path::new("/tmp/sabiql-mysql.cnf"));

        assert_eq!(
            args,
            vec![
                "--defaults-file=/tmp/sabiql-mysql.cnf",
                "--no-login-paths",
                "--protocol=TCP",
                "--connect-timeout=10",
                "--skip-reconnect",
                "--xml",
                "--binary-as-hex",
                "--binary-mode",
                "--unbuffered",
                "--default-character-set=utf8mb4",
                "--batch",
                "--silent",
                "--prompt=",
            ]
        );
        assert!(!args.iter().any(|argument| argument == "--quick"));
        assert!(
            args.iter()
                .all(|argument| !argument.starts_with("--max-allowed-packet="))
        );
    }

    #[test]
    fn adhoc_arguments_enable_statement_diagnostics_without_changing_query_options() {
        let args = mysql_adhoc_args(Path::new("/tmp/sabiql-mysql.cnf"));

        assert_eq!(args.last().map(String::as_str), Some("--show-warnings"));
        assert_eq!(
            args.iter().filter(|arg| *arg == "--show-warnings").count(),
            1
        );
        assert!(
            mysql_query_args(Path::new("/tmp/sabiql-mysql.cnf"))
                .iter()
                .all(|argument| argument != "--show-warnings")
        );
        assert!(
            mysql_metadata_args(Path::new("/tmp/sabiql-mysql.cnf"))
                .iter()
                .all(|argument| argument != "--show-warnings")
        );
    }
}
