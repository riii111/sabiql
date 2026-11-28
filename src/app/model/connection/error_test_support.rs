use super::ConnectionErrorInfo;
use crate::policy::password_masking::mask_password;

pub fn from_parts(
    summary: &'static str,
    hint: &'static str,
    retryable: bool,
    raw_stderr: impl Into<String>,
) -> ConnectionErrorInfo {
    ConnectionErrorInfo {
        summary,
        hint,
        retryable,
        masked_details: mask_password(&raw_stderr.into()),
    }
}
