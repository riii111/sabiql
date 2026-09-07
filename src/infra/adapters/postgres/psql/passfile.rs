use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use uuid::Uuid;

use crate::app::ports::outbound::DbOperationError;

pub(super) struct Passfile {
    pub(super) path: PathBuf,
}

impl Passfile {
    pub(super) fn create(password: &str) -> Result<Self, DbOperationError> {
        // libpq reads one physical line; escaping LF cannot preserve the password.
        if password.contains(['\n', '\0']) {
            return Err(DbOperationError::ConnectionFailed(
                "PostgreSQL passwords containing LF or NUL cannot be passed through a password file"
                    .to_string(),
            ));
        }
        let path = std::path::absolute(
            std::env::temp_dir().join(format!("sabiql-pg-{}.pgpass", Uuid::new_v4())),
        )
        .map_err(passfile_error)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        #[cfg(windows)]
        std::os::windows::fs::OpenOptionsExt::access_mode(
            &mut options,
            windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE
                | windows_sys::Win32::Storage::FileSystem::WRITE_DAC,
        );
        let file = options.open(&path).map_err(passfile_error)?;
        let passfile = Self { path };
        write_password(file, password).map_err(passfile_error)?;
        Ok(passfile)
    }
}

impl Drop for Passfile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn write_password(mut file: File, password: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    crate::adapters::windows_file_security::restrict_to_current_user(&file)?;
    let escaped = password.replace('\\', "\\\\").replace(':', "\\:");
    // A per-process wildcard retains libpq's socket, host list and default-user semantics.
    // The final delimiter prevents libpq from stripping a password's trailing CR.
    writeln!(file, "*:*:*:*:{escaped}:")
}

fn passfile_error(error: std::io::Error) -> DbOperationError {
    DbOperationError::ConnectionFailed(format!(
        "Unable to prepare PostgreSQL password file: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_password_and_preserves_trailing_carriage_return() {
        let file = Passfile::create("p:a\\ss '日本語'\t\r").unwrap();
        let path = file.path.clone();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "*:*:*:*:p\\:a\\\\ss '日本語'\t\r:\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        #[cfg(windows)]
        crate::adapters::windows_file_security::test_support::assert_owner_only_acl(&path);
        drop(file);
        assert!(!path.exists());
    }

    #[test]
    fn nul_is_rejected_without_echoing_password() {
        let error = Passfile::create("private\0value").err().unwrap();

        assert!(!error.to_string().contains("private"));
        assert!(matches!(error, DbOperationError::ConnectionFailed(_)));
    }
}
