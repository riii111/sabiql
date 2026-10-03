use std::io::{BufRead, BufReader, Read, Write};
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{SecretStoreError, entry, map_keyring_error};

const HELPER_ARGUMENT: &str = "--sabiql-secret-store-helper";
// Allows time to find and answer an OS unlock prompt, while bounding startup.
const OPERATION_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub(super) enum Request {
    Get { reference: String },
    Set { reference: String, secret: String },
    Delete { reference: String },
}

type Response = Result<String, SecretStoreError>;

// Called before constructing the application's runtime or terminal.
#[allow(
    clippy::exit,
    reason = "isolated helper must stop a blocked OS call when its parent exits"
)]
pub fn run_secret_store_helper() -> bool {
    if std::env::args_os().skip(1).collect::<Vec<_>>() != [HELPER_ARGUMENT] {
        return false;
    }
    let mut input = BufReader::new(std::io::stdin());
    let request = read_message::<Request>(&mut input);
    // The parent keeps stdin open for the operation's entire lifetime. EOF also
    // handles abrupt parent exit on platforms without a parent-death signal.
    std::thread::spawn(move || {
        wait_for_parent_disconnect(&mut input);
        std::process::exit(1);
    });
    let response = request.and_then(execute);
    let _ = write_message(&mut std::io::stdout().lock(), &response);
    true
}

fn wait_for_parent_disconnect(input: &mut impl Read) {
    let _ = std::io::copy(input, &mut std::io::sink());
}

pub(super) fn call(request: Request) -> Response {
    #[cfg(target_os = "linux")]
    let executable = linux_helper_executable();
    #[cfg(not(target_os = "linux"))]
    let executable = std::env::current_exe().map_err(|_| SecretStoreError::OperationFailed)?;
    let mut command = Command::new(executable);
    command.arg(HELPER_ARGUMENT);
    call_command(command, request, OPERATION_TIMEOUT)
}

#[cfg(target_os = "linux")]
fn linux_helper_executable() -> PathBuf {
    // A package update can unlink the original pathname while this image is running.
    PathBuf::from("/proc/self/exe")
}

fn call_command(mut command: Command, request: Request, timeout: Duration) -> Response {
    // Only the fixed helper selector appears in argv. Secrets travel through pipes.
    let mut child = ChildGuard(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| SecretStoreError::OperationFailed)?,
    );
    let mut input = child
        .0
        .stdin
        .take()
        .ok_or(SecretStoreError::OperationFailed)?;
    let output = child
        .0
        .stdout
        .take()
        .ok_or(SecretStoreError::OperationFailed)?;
    let (sender, receiver) = mpsc::channel();
    // Plain threads do not hold Tokio shutdown open. Killing the isolated helper
    // closes both pipes and releases this thread even if the platform API hangs.
    std::thread::spawn(move || {
        let response = write_message(&mut input, &request)
            .and_then(|()| read_message::<Response>(&mut BufReader::new(output)))
            .and_then(|response| response);
        let _ = sender.send(response);
        drop(input);
    });
    match receiver.recv_timeout(timeout) {
        Ok(response) => response,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(SecretStoreError::TimedOut),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(SecretStoreError::OperationFailed),
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn execute(request: Request) -> Response {
    match request {
        Request::Get { reference } => entry(&reference)?.get_password().map_err(map_keyring_error),
        Request::Set { reference, secret } => entry(&reference)?
            .set_password(&secret)
            .map(|()| String::new())
            .map_err(map_keyring_error),
        Request::Delete { reference } => entry(&reference)?
            .delete_credential()
            .map(|()| String::new())
            .map_err(map_keyring_error),
    }
}

fn write_message(
    writer: &mut impl Write,
    message: &impl Serialize,
) -> Result<(), SecretStoreError> {
    let mut bytes = serde_json::to_vec(message).map_err(|_| SecretStoreError::OperationFailed)?;
    if bytes.len() as u64 >= MAX_MESSAGE_BYTES {
        return Err(SecretStoreError::OperationFailed);
    }
    bytes.push(b'\n');
    writer
        .write_all(&bytes)
        .and_then(|()| writer.flush())
        .map_err(|_| SecretStoreError::OperationFailed)
}

fn read_message<T: serde::de::DeserializeOwned>(
    reader: &mut impl BufRead,
) -> Result<T, SecretStoreError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_MESSAGE_BYTES)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| SecretStoreError::OperationFailed)?;
    if bytes.last() != Some(&b'\n') {
        return Err(SecretStoreError::OperationFailed);
    }
    serde_json::from_slice(&bytes).map_err(|_| SecretStoreError::OperationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn helper_reexecutes_running_image_after_its_path_is_unlinked() {
        const CHILD_FLAG: &str = "SABIQL_UNLINKED_HELPER_TEST_CHILD";
        if let Some(copy_path) = std::env::var_os(CHILD_FLAG) {
            // Only the disposable copy made by this test is unlinked.
            let executable = std::env::current_exe().unwrap();
            assert_eq!(executable, PathBuf::from(copy_path));
            std::fs::remove_file(executable).unwrap();
            let result = Command::new(linux_helper_executable())
                .arg("--list")
                .output()
                .unwrap();
            assert!(result.status.success());
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("fixture-test");
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        let result = Command::new(&executable)
            .args([
                "--exact",
                "adapters::secret_store::process::tests::helper_reexecutes_running_image_after_its_path_is_unlinked",
                "--nocapture",
            ])
            .env(CHILD_FLAG, &executable)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!executable.exists());
    }

    #[cfg(unix)]
    #[test]
    fn unresponsive_helper_is_killed_within_deadline_for_every_operation() {
        for request in [
            Request::Get {
                reference: "fixture".into(),
            },
            Request::Set {
                reference: "fixture".into(),
                secret: "fake password".into(),
            },
            Request::Delete {
                reference: "fixture".into(),
            },
        ] {
            let mut command = Command::new("sh");
            command.args(["-c", "exec sleep 30"]);
            let start = std::time::Instant::now();

            let response = call_command(command, request, Duration::from_millis(40));

            assert_eq!(response, Err(SecretStoreError::TimedOut));
            assert!(start.elapsed() < Duration::from_secs(2));
        }
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_detects_parent_pipe_close_without_waiting_for_operation() {
        let (mut parent, mut helper) = std::os::unix::net::UnixStream::pair().unwrap();
        let (sender, receiver) = mpsc::channel();
        let watcher = std::thread::spawn(move || {
            wait_for_parent_disconnect(&mut helper);
            sender.send(()).unwrap();
        });
        parent.write_all(b"still alive").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(20)).is_err());

        drop(parent);

        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        watcher.join().unwrap();
    }

    #[test]
    fn protocol_roundtrips_newlines_without_exposing_them_as_commands() {
        let request = Request::Set {
            reference: "fixture".into(),
            secret: "fake\nsecret".into(),
        };
        let mut bytes = Vec::new();

        write_message(&mut bytes, &request).unwrap();
        let parsed: Request = read_message(&mut &bytes[..]).unwrap();

        assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert!(matches!(parsed, Request::Set { secret, .. } if secret == "fake\nsecret"));
    }
}
