pub(super) fn mysql_query_args(option_file: &std::path::Path) -> Vec<String> {
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
pub(super) fn mysql_metadata_args(option_file: &std::path::Path) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_keep_credentials_out_of_argv() {
        let args = mysql_query_args(std::path::Path::new("/tmp/sabiql-mysql.cnf"));

        assert_eq!(args[0], "--defaults-file=/tmp/sabiql-mysql.cnf");
        assert_eq!(args[1], "--no-login-paths");
        for expected in [
            "--xml",
            "--binary-as-hex",
            "--binary-mode",
            "--unbuffered",
            "--skip-reconnect",
            "--default-character-set=utf8mb4",
        ] {
            assert!(args.contains(&expected.to_string()), "{expected}");
        }
        assert!(args.contains(&"--batch".to_string()));
        assert!(args.iter().all(|argument| !argument.contains("password")));
    }

    #[test]
    fn metadata_arguments_request_column_names_for_empty_results() {
        let args = mysql_metadata_args(std::path::Path::new("/tmp/sabiql-mysql.cnf"));

        assert!(args.contains(&"--column-names".to_string()));
        assert!(args.contains(&"--column-type-info".to_string()));
    }
}
