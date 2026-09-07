use std::io::{IsTerminal, Write, stdout};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::app::ports::outbound::clipboard::{ClipboardError, ClipboardOutcome, ClipboardWriter};

pub(super) struct ArboardClipboard;

pub(super) struct Osc52Clipboard;

// Bound the encoded sequence below 100 KB without truncating the source value.
const MAX_OSC52_BYTES: usize = 74_994;

impl ClipboardWriter for ArboardClipboard {
    fn copy_text(&self, content: &str) -> Result<ClipboardOutcome, ClipboardError> {
        copy_native(content).map(|()| ClipboardOutcome::Copied)
    }
}

impl ClipboardWriter for Osc52Clipboard {
    fn copy_text(&self, content: &str) -> Result<ClipboardOutcome, ClipboardError> {
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
