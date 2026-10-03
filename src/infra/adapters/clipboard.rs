use std::io::{IsTerminal, Write, stdout};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::app::model::shared::settings::ClipboardBackend;
use crate::app::ports::outbound::clipboard::{ClipboardError, ClipboardOutcome, ClipboardWriter};

pub(super) struct RoutingClipboard {
    ssh: bool,
    native: Box<dyn ClipboardWriter>,
    terminal: Box<dyn ClipboardWriter>,
}

impl RoutingClipboard {
    pub(super) fn from_environment() -> Self {
        Self {
            ssh: is_ssh(
                std::env::var_os("SSH_CONNECTION").as_deref(),
                std::env::var_os("SSH_TTY").as_deref(),
            ),
            native: Box::new(ArboardClipboard),
            terminal: Box::new(Osc52Clipboard),
        }
    }
}

fn is_ssh(connection: Option<&std::ffi::OsStr>, tty: Option<&std::ffi::OsStr>) -> bool {
    [connection, tty]
        .into_iter()
        .flatten()
        .any(|value| !value.is_empty())
}

impl ClipboardWriter for RoutingClipboard {
    fn copy_text(
        &self,
        content: &str,
        backend: ClipboardBackend,
    ) -> Result<ClipboardOutcome, ClipboardError> {
        match backend {
            ClipboardBackend::Native => self.native.copy_text(content, backend),
            ClipboardBackend::Osc52 => self.terminal.copy_text(content, backend),
            ClipboardBackend::Auto if self.ssh => self.terminal.copy_text(content, backend),
            ClipboardBackend::Auto => self.native.copy_text(content, backend).or_else(|_| {
                self.terminal.copy_text(content, backend).map_err(|_| ClipboardError::Terminal(
                    "OS clipboard and OSC 52 copy failed; check terminal clipboard permissions and copy size".into()
                ))
            }),
        }
    }
}

pub(super) struct ArboardClipboard;

pub(super) struct Osc52Clipboard;

// Bound the encoded sequence below 100 KB without truncating the source value.
const MAX_OSC52_BYTES: usize = 74_994;

impl ClipboardWriter for ArboardClipboard {
    fn copy_text(
        &self,
        content: &str,
        _backend: ClipboardBackend,
    ) -> Result<ClipboardOutcome, ClipboardError> {
        copy_native(content).map(|()| ClipboardOutcome::Copied)
    }
}

impl ClipboardWriter for Osc52Clipboard {
    fn copy_text(
        &self,
        content: &str,
        _backend: ClipboardBackend,
    ) -> Result<ClipboardOutcome, ClipboardError> {
        let stdout = stdout();
        if !stdout.is_terminal() {
            return Err(ClipboardError::Terminal(
                "OSC 52 requires terminal stdout".into(),
            ));
        }
        // The renderer holds this same stdout lock for its entire frame.
        write_osc52(content, &mut stdout.lock())
    }
}

fn write_osc52(content: &str, output: &mut impl Write) -> Result<ClipboardOutcome, ClipboardError> {
    if content.len() > MAX_OSC52_BYTES {
        return Err(ClipboardError::Terminal(format!(
            "OSC 52 copy exceeds {MAX_OSC52_BYTES} UTF-8 bytes; nothing sent"
        )));
    }
    let sequence = format!("\x1b]52;c;{}\x07", STANDARD.encode(content));
    output
        .write_all(sequence.as_bytes())
        .and_then(|()| output.flush())
        .map_err(|error| ClipboardError::Terminal(format!("OSC 52 output failed: {error}")))?;
    Ok(ClipboardOutcome::SentToTerminal)
}

#[cfg(not(target_os = "android"))]
fn copy_native(content: &str) -> Result<(), ClipboardError> {
    arboard::Clipboard::new()
        .and_then(|mut cb| cb.set_text(content))
        .map_err(ClipboardError::backend)
}

#[cfg(target_os = "android")]
fn copy_native(_content: &str) -> Result<(), ClipboardError> {
    Err(ClipboardError::Unavailable("Clipboard unavailable".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct RecordingClipboard {
        calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        outcome: Result<ClipboardOutcome, ClipboardError>,
    }

    impl ClipboardWriter for RecordingClipboard {
        fn copy_text(
            &self,
            content: &str,
            _: ClipboardBackend,
        ) -> Result<ClipboardOutcome, ClipboardError> {
            self.calls.lock().unwrap().push(content.to_owned());
            self.outcome.clone()
        }
    }

    #[rstest::rstest]
    #[case(None, None, false)]
    #[case(Some(""), Some(""), false)]
    #[case(Some("remote"), None, true)]
    #[case(None, Some("/dev/pts/1"), true)]
    fn ssh_detection_requires_nonempty_environment(
        #[case] connection: Option<&str>,
        #[case] tty: Option<&str>,
        #[case] expected: bool,
    ) {
        assert_eq!(
            is_ssh(
                connection.map(std::ffi::OsStr::new),
                tty.map(std::ffi::OsStr::new)
            ),
            expected
        );
    }

    #[rstest::rstest]
    #[case(
        ClipboardBackend::Auto,
        true,
        true,
        true,
        0,
        1,
        Some(ClipboardOutcome::SentToTerminal)
    )]
    #[case(
        ClipboardBackend::Auto,
        false,
        true,
        true,
        1,
        0,
        Some(ClipboardOutcome::Copied)
    )]
    #[case(
        ClipboardBackend::Auto,
        false,
        false,
        true,
        1,
        1,
        Some(ClipboardOutcome::SentToTerminal)
    )]
    #[case(ClipboardBackend::Auto, false, false, false, 1, 1, None)]
    #[case(
        ClipboardBackend::Native,
        true,
        true,
        true,
        1,
        0,
        Some(ClipboardOutcome::Copied)
    )]
    #[case(ClipboardBackend::Native, false, false, true, 1, 0, None)]
    #[case(
        ClipboardBackend::Osc52,
        false,
        true,
        true,
        0,
        1,
        Some(ClipboardOutcome::SentToTerminal)
    )]
    fn routing_calls_only_required_backends(
        #[case] backend: ClipboardBackend,
        #[case] ssh: bool,
        #[case] native_ok: bool,
        #[case] terminal_ok: bool,
        #[case] native_count: usize,
        #[case] terminal_count: usize,
        #[case] expected: Option<ClipboardOutcome>,
    ) {
        let native_calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let terminal_calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let router = RoutingClipboard {
            ssh,
            native: Box::new(RecordingClipboard {
                calls: native_calls.clone(),
                outcome: if native_ok {
                    Ok(ClipboardOutcome::Copied)
                } else {
                    Err(ClipboardError::Unavailable("secret".into()))
                },
            }),
            terminal: Box::new(RecordingClipboard {
                calls: terminal_calls.clone(),
                outcome: if terminal_ok {
                    Ok(ClipboardOutcome::SentToTerminal)
                } else {
                    Err(ClipboardError::Terminal("failure".into()))
                },
            }),
        };

        let result = router.copy_text("secret", backend);

        assert_eq!(result.as_ref().ok().copied(), expected);
        assert_eq!(native_calls.lock().unwrap().len(), native_count);
        assert_eq!(terminal_calls.lock().unwrap().len(), terminal_count);
        if backend == ClipboardBackend::Auto && expected.is_none() {
            assert!(!result.unwrap_err().to_string().contains("secret"));
        }
    }

    #[rstest::rstest]
    #[case("", "")]
    #[case("f", "Zg==")]
    #[case("fo", "Zm8=")]
    #[case("foo", "Zm9v")]
    #[case("hello", "aGVsbG8=")]
    #[case("日本語🙂", "5pel5pys6Kqe8J+Zgg==")]
    #[case("\x1b]52;c;?\x07\n\t\0", "G101MjtjOz8HCgkA")]
    fn encodes_original_utf8_without_raw_control_characters(
        #[case] content: &str,
        #[case] encoded: &str,
    ) {
        let mut output = Vec::new();

        let outcome = write_osc52(content, &mut output).unwrap();

        assert_eq!(outcome, ClipboardOutcome::SentToTerminal);
        assert_eq!(output, format!("\x1b]52;c;{encoded}\x07").as_bytes());
    }

    #[test]
    fn accepts_limit_and_rejects_next_utf8_byte_without_output() {
        let content = "日".repeat(MAX_OSC52_BYTES / 3);
        let mut output = Vec::new();

        write_osc52(&content, &mut output).unwrap();

        assert_eq!(output.len(), 100_000);
        assert_eq!(
            STANDARD.decode(&output[7..output.len() - 1]).unwrap(),
            content.as_bytes()
        );
        output.clear();
        assert!(write_osc52(&(content + "a"), &mut output).is_err());
        assert!(output.is_empty());
    }

    #[rstest::rstest]
    #[case(false)]
    #[case(true)]
    fn output_failure_never_reports_sent(#[case] fail_flush: bool) {
        let mut output = FailingOutput { fail_flush };

        let error = write_osc52("secret", &mut output).unwrap_err();

        assert!(error.to_string().contains("OSC 52 output failed"));
        assert!(!error.to_string().contains("secret"));
    }

    #[test]
    fn retries_short_writes_until_sequence_is_complete() {
        let mut output = ShortOutput(Vec::new());

        write_osc52("hello", &mut output).unwrap();

        assert_eq!(output.0, b"\x1b]52;c;aGVsbG8=\x07");
    }

    struct FailingOutput {
        fail_flush: bool,
    }

    impl Write for FailingOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_flush {
                Ok(bytes.len())
            } else {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    struct ShortOutput(Vec<u8>);

    impl Write for ShortOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let length = bytes.len().min(2);
            self.0.extend_from_slice(&bytes[..length]);
            Ok(length)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
