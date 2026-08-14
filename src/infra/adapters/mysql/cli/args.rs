use super::MYSQL_PROBE_QUERY;

pub(crate) fn mysql_probe_args(option_file: &std::path::Path) -> Vec<String> {
    vec![
        format!("--defaults-file={}", option_file.display()),
        "--no-login-paths".to_string(),
        "--protocol=TCP".to_string(),
        "--connect-timeout=10".to_string(),
        "--batch".to_string(),
        "--raw".to_string(),
        "--skip-column-names".to_string(),
        "--binary-mode".to_string(),
        "--skip-reconnect".to_string(),
        format!("--execute={MYSQL_PROBE_QUERY}"),
    ]
}

pub(crate) fn mysql_query_args(option_file: &std::path::Path) -> Vec<String> {
    vec![
        format!("--defaults-file={}", option_file.display()),
        "--no-login-paths".to_string(),
        "--protocol=TCP".to_string(),
        "--connect-timeout=10".to_string(),
        "--xml".to_string(),
        "--binary-as-hex".to_string(),
        "--binary-mode".to_string(),
        "--unbuffered".to_string(),
        "--skip-reconnect".to_string(),
        "--default-character-set=utf8mb4".to_string(),
        "--batch".to_string(),
        "--silent".to_string(),
        "--prompt=".to_string(),
    ]
}

pub(crate) fn mysql_metadata_args(option_file: &std::path::Path) -> Vec<String> {
    vec![
        format!("--defaults-file={}", option_file.display()),
        "--no-login-paths".to_string(),
        "--protocol=TCP".to_string(),
        "--connect-timeout=10".to_string(),
        "--batch".to_string(),
        "--column-names".to_string(),
        "--column-type-info".to_string(),
        "--binary-as-hex".to_string(),
        "--binary-mode".to_string(),
        "--unbuffered".to_string(),
        "--skip-reconnect".to_string(),
        "--default-character-set=utf8mb4".to_string(),
        "--prompt=".to_string(),
    ]
}
