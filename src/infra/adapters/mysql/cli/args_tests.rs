#[cfg(test)]
mod args_tests {
    use super::super::*;

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
}
