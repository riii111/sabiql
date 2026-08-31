use super::ConnectionErrorInfo;

pub fn from_parts(
    summary: &'static str,
    hint: &'static str,
    retryable: bool,
    raw_stderr: impl Into<String>,
) -> ConnectionErrorInfo {
    ConnectionErrorInfo::from_presentation(summary, hint, retryable, raw_stderr)
}
