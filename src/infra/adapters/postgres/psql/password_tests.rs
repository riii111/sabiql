use super::*;
use crate::app::ports::outbound::DsnBuilder;
use crate::domain::connection::{ConnectionProfile, SslMode};

fn profile_dsn(host: &str, password: &str) -> String {
    let profile = ConnectionProfile::new_postgres(
        "test",
        host,
        5432,
        "db name",
        "user'名",
        password,
        SslMode::Disable,
    )
    .unwrap();
    PostgresAdapter::new().build_dsn(&profile)
}

#[test]
fn generated_password_is_absent_from_argv_for_tcp_socket_and_ipv6() {
    for host in ["localhost", "/tmp/pg socket", "::1"] {
        let dsn = profile_dsn(host, "private :\\' 日本語\rend");
        let (cmd, passfile) =
            PostgresAdapter::build_psql_command(&dsn, &["--csv"], &["-c", "SELECT 1"], true)
                .unwrap();
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy())
            .collect();

        assert!(!args.iter().any(|arg| arg.contains("private")));
        assert!(args[0].contains("password=''"));
        assert!(args[0].contains(host));
        let path = passfile.as_ref().unwrap().path.to_str().unwrap();
        assert!(args[0].ends_with(&format!("passfile={}", quote_conninfo_value(path))));
        assert!(
            cmd.as_std()
                .get_envs()
                .any(|(key, value)| key == "PGPASSWORD" && value.is_none())
        );
    }
}

#[test]
fn password_text_inside_another_field_is_not_extracted() {
    let dsn = "host='host password=not-a-secret' dbname='db'";
    let (cmd, file) = PostgresAdapter::build_psql_command(dsn, &[], &[], false).unwrap();

    assert!(file.is_none());
    assert_eq!(cmd.as_std().get_args().next().unwrap(), dsn);
}

#[test]
fn uri_password_is_absent_from_argv_and_uses_a_temporary_passfile() {
    for scheme in ["postgres", "postgresql"] {
        let dsn = format!("{scheme}://user:p%40ss%3Aword@localhost/db?sslmode=require");
        let (cmd, passfile) =
            PostgresAdapter::build_psql_command(&dsn, &[], &["-c", "SELECT 1"], false).unwrap();
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            args[0],
            format!("{scheme}://user@localhost/db?sslmode=require")
        );
        assert!(
            !args
                .iter()
                .any(|arg| arg.contains("p%40ss") || arg.contains("word"))
        );
        let path = passfile.as_ref().unwrap().path.to_str().unwrap();
        assert!(
            cmd.as_std()
                .get_envs()
                .any(|(key, value)| key == "PGPASSFILE" && value == Some(path.as_ref()))
        );
        assert!(
            cmd.as_std()
                .get_envs()
                .any(|(key, value)| key == "PGPASSWORD" && value.is_none())
        );
    }
}

#[test]
fn libpq_uri_password_is_absent_from_argv_when_url_parser_rejects_uri() {
    for dsn in [
        "postgresql://user:secret@/db?host=/var/run/postgresql",
        "postgresql://user:secret@host1,host2/db",
        "postgresql://user@host/db?pass%77ord=secret",
    ] {
        let (cmd, passfile) =
            PostgresAdapter::build_psql_command(dsn, &[], &["-c", "SELECT 1"], false).unwrap();
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();

        assert!(!args.iter().any(|arg| arg.contains("secret")));
        assert!(args[0].contains("user@"));
        assert!(passfile.is_some());
    }
}

#[test]
fn query_password_uses_last_non_empty_value() {
    let dsn = "postgresql://user:first@host/db?password=second&pass%77ord=third";
    let (cmd, passfile) =
        PostgresAdapter::build_psql_command(dsn, &[], &["-c", "SELECT 1"], false).unwrap();
    let args: Vec<_> = cmd
        .as_std()
        .get_args()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    let password_file = passfile.as_ref().unwrap().path.to_str().unwrap();
    let password_file_contents = std::fs::read_to_string(password_file).unwrap();

    assert!(
        !args.iter().any(|arg| {
            arg.contains("first") || arg.contains("second") || arg.contains("third")
        })
    );
    assert!(password_file_contents.contains("third"));
}

#[test]
fn invalid_uri_password_encoding_fails_without_echoing_uri() {
    let dsn = "postgresql://user:%ZZ@localhost/db";
    let Err(error) = PostgresAdapter::build_psql_command(dsn, &[], &[], false) else {
        panic!("invalid URI password encoding should fail before spawn")
    };

    assert!(
        matches!(error, DbOperationError::ConnectionFailed(ref message) if message == "Invalid PostgreSQL URI password encoding")
    );
    assert!(!error.to_string().contains("%ZZ"));
}

#[test]
fn uri_sslpassword_is_rejected_without_echoing_secret() {
    for dsn in [
        "postgresql://user@host/db?sslpassword=secret",
        "postgresql://user@host/db?sslpass%77ord=secret",
    ] {
        let Err(error) = PostgresAdapter::build_psql_command(dsn, &[], &[], false) else {
            panic!("URI sslpassword should be rejected before spawn")
        };

        assert!(
            matches!(error, DbOperationError::ConnectionFailed(ref message) if message == "PostgreSQL URI sslpassword cannot be passed securely to psql")
        );
        assert!(!error.to_string().contains("secret"));
    }
}

#[test]
fn no_explicit_password_preserves_service_passfile_and_environment() {
    for dsn in [
        "service=mydb",
        "host='localhost' passfile='/existing/file'",
        "host='localhost' password=''",
        "port='5432' dbname='db'",
    ] {
        let (cmd, file) = PostgresAdapter::build_psql_command(dsn, &[], &[], false).unwrap();

        assert!(file.is_none());
        assert_eq!(cmd.as_std().get_args().next().unwrap(), dsn);
        assert!(
            !cmd.as_std()
                .get_envs()
                .any(|(key, _)| key == "PGPASSWORD" || key == "PGPASSFILE")
        );
    }
}

#[cfg(unix)]
fn fake_command(script: &str) -> (Command, Option<Passfile>, std::path::PathBuf) {
    let dsn = profile_dsn("localhost", "private :\\' 日本語");
    let (real, passfile) =
        PostgresAdapter::build_psql_command(&dsn, &[], &["-c", "SELECT 1"], false).unwrap();
    let path = passfile.as_ref().unwrap().path.clone();
    let mut fake = Command::new("sh");
    fake.args(["-c", script, "fake-psql"]);
    fake.args(real.as_std().get_args());
    fake.env("TEST_PASSFILE", &path);
    (fake, passfile, path)
}

#[cfg(unix)]
#[tokio::test]
async fn fake_cli_observes_safe_argv_and_file_is_removed_after_success_or_failure() {
    for status in [0, 7] {
        let script = format!(
            "case \"$*\" in *private*) exit 99;; esac; test -r \"$TEST_PASSFILE\" || exit 98; printf ok; exit {status}"
        );
        let (mut cmd, file, path) = fake_command(&script);

        let result = PostgresAdapter::collect_output(&mut cmd, file, 2).await;

        if status == 0 {
            assert_eq!(result.unwrap(), "ok");
        } else {
            assert!(
                matches!(result, Err(DbOperationError::QueryFailed(message)) if message == "psql exited with status code 7")
            );
        }
        assert!(!path.exists());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn spawn_failure_removes_passfile() {
    let (_, file, path) = fake_command("exit 0");
    let mut cmd = Command::new("/nonexistent/sabiql-cf02-psql");

    assert!(
        PostgresAdapter::collect_output(&mut cmd, file, 1)
            .await
            .is_err()
    );
    assert!(!path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_reaps_child_and_removes_passfile() {
    let (mut cmd, file, path) = fake_command("exec sleep 30");

    let result = PostgresAdapter::collect_output(&mut cmd, file, 0).await;

    assert!(matches!(result, Err(DbOperationError::Timeout(_))));
    assert!(!path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_reaps_child_before_removing_passfile() {
    let (mut cmd, file, path) = fake_command("printf ready; exec sleep 30");
    let mut process = PsqlProcess::spawn(&mut cmd, file).unwrap();
    let child = process.child.as_mut().unwrap();
    let pid = child.id().unwrap();
    let mut ready = [0; 5];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut ready)
        .await
        .unwrap();
    assert_eq!(&ready, b"ready");
    let task = tokio::spawn(async move {
        process.child.as_mut().unwrap().wait().await.unwrap();
    });

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(2), async {
        while path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn csv_cancellation_removes_passfile_and_reaps_child() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("export.csv");
    let (mut cmd, file, path) = fake_command("printf ready; exec sleep 30");
    let mut process = PsqlProcess::spawn(&mut cmd, file).unwrap();
    let child = process.child.as_mut().unwrap();
    let pid = child.id().unwrap();
    let mut ready = [0; 5];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut ready)
        .await
        .unwrap();
    let task =
        tokio::spawn(
            async move { collect_csv_output(process, &output, Duration::from_secs(30)).await },
        );
    tokio::task::yield_now().await;

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(2), async {
        while path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn csv_success_failure_and_timeout_remove_passfile() {
    let dir = tempfile::tempdir().unwrap();
    for (script, duration, succeeds) in [
        ("printf 'id\\n1\\n'", Duration::from_secs(2), true),
        ("exit 7", Duration::from_secs(2), false),
        ("exec sleep 30", Duration::ZERO, false),
    ] {
        let (mut cmd, file, path) = fake_command(script);
        let process = PsqlProcess::spawn(&mut cmd, file).unwrap();
        let output = dir.path().join("export.csv");

        let result = collect_csv_output(process, &output, duration).await;

        assert_eq!(result.is_ok(), succeeds);
        assert!(!path.exists());
        if succeeds {
            assert_eq!(std::fs::read_to_string(output).unwrap(), "id\n1\n");
        }
    }
}

#[tokio::test]
#[ignore = "requires psql and a loopback listener; no database required"]
async fn real_libpq_preserves_special_passwords_and_explicit_precedence() {
    for password in [
        "p:a\\ss '日本語'\t",
        "middle\rcarriage",
        "last\\",
        &"long:秘密\\".repeat(1000),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let start = std::time::Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            start.elapsed() < Duration::from_secs(5),
                            "psql did not connect"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut size = [0; 4];
            stream.read_exact(&mut size).unwrap();
            let mut startup = vec![0; u32::from_be_bytes(size) as usize - 4];
            stream.read_exact(&mut startup).unwrap();
            stream.write_all(&[b'R', 0, 0, 0, 8, 0, 0, 0, 3]).unwrap();
            let mut tag = [0; 1];
            stream.read_exact(&mut tag).unwrap();
            assert_eq!(tag, [b'p']);
            stream.read_exact(&mut size).unwrap();
            let mut supplied = vec![0; u32::from_be_bytes(size) as usize - 4];
            stream.read_exact(&mut supplied).unwrap();
            assert_eq!(supplied.pop(), Some(0));
            String::from_utf8(supplied).unwrap()
        });
        let profile = ConnectionProfile::new_postgres(
            "test",
            "127.0.0.1",
            port,
            "db",
            "user",
            password,
            SslMode::Disable,
        )
        .unwrap();
        let dsn = PostgresAdapter::new().build_dsn(&profile);
        let (mut cmd, file) =
            PostgresAdapter::build_psql_command(&dsn, &["-w"], &["-c", "SELECT 1"], false).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let service = dir.path().join("pg_service.conf");
        std::fs::write(
            &service,
            "[test]\npassword=wrong-service-password\npassfile=/nonexistent/service-passfile\n",
        )
        .unwrap();
        cmd.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("PGGSSENCMODE", "disable")
            .env("PGSERVICEFILE", service)
            .env("PGSERVICE", "test")
            .env("PGPASSWORD", "wrong-environment-password")
            .env("PGPASSFILE", "/nonexistent/environment-passfile");
        let path = file.as_ref().unwrap().path.clone();

        // The fixture closes after authentication; it never executes SQL.
        assert!(
            PostgresAdapter::collect_output(&mut cmd, file, 5)
                .await
                .is_err()
        );

        assert_eq!(server.join().unwrap(), password);
        assert!(!path.exists());
    }
}
