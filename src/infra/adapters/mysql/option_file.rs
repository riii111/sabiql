#[cfg(windows)]
use std::fs::File;
use std::fs::{self, OpenOptions};
#[cfg(windows)]
use std::io;
use std::io::Write;
use std::path::PathBuf;

use uuid::Uuid;

use crate::app::ports::outbound::DbOperationError;

use super::dsn::{MySqlDsn, validate_mysql_tls_files, validate_mysql_values};

pub(super) struct MySqlOptionFile {
    pub(super) path: PathBuf,
}

impl MySqlOptionFile {
    pub(super) fn create(target: &MySqlDsn) -> Result<Self, DbOperationError> {
        validate_mysql_values(target)?;
        validate_mysql_tls_files(target)?;
        let mut path = std::env::temp_dir();
        path.push(format!("sabiql-mysql-{}.cnf", Uuid::new_v4()));
        if !path.is_absolute() {
            path = std::env::current_dir()
                .map_err(|error| DbOperationError::ConnectionFailed(error.to_string()))?
                .join(path);
        }

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
        let mut file = options.open(&path).map_err(|error| {
            DbOperationError::ConnectionFailed(format!(
                "Unable to create MySQL option file: {error}"
            ))
        })?;
        #[cfg(windows)]
        if let Err(error) = set_file_permissions(&file) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(DbOperationError::ConnectionFailed(format!(
                "Unable to secure MySQL option file: {error}"
            )));
        }
        let contents = serialize_option_file(target);
        if let Err(error) = file.write_all(contents.as_bytes()) {
            let _ = fs::remove_file(&path);
            return Err(DbOperationError::ConnectionFailed(format!(
                "Unable to write MySQL option file: {error}"
            )));
        }
        Ok(Self { path })
    }
}

#[cfg(windows)]
fn set_file_permissions(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, GRANT_ACCESS, SetEntriesInAclW, SetSecurityInfo, TRUSTEE_IS_SID,
        TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, NO_INHERITANCE, PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token: HANDLE = null_mut();
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }

    let sid_result = current_user_sid(token);
    unsafe {
        CloseHandle(token);
    }
    let mut sid = sid_result?;

    let trustee = TRUSTEE_W {
        pMultipleTrustee: null_mut(),
        MultipleTrusteeOperation: 0,
        TrusteeForm: TRUSTEE_IS_SID,
        TrusteeType: TRUSTEE_IS_USER,
        ptstrName: sid.as_mut_ptr().cast(),
    };
    let access = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_GENERIC_READ | FILE_GENERIC_WRITE,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: trustee,
    };
    let mut acl = null_mut();
    let status = unsafe { SetEntriesInAclW(1, &raw const access, null(), &raw mut acl) };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }

    let status = unsafe {
        SetSecurityInfo(
            file.as_raw_handle(),
            windows_sys::Win32::Security::Authorization::SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            acl,
            null(),
        )
    };
    unsafe {
        LocalFree(acl.cast());
    }
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }

    Ok(())
}

#[cfg(windows)]
fn current_user_sid(token: windows_sys::Win32::Foundation::HANDLE) -> io::Result<Vec<u8>> {
    use std::ptr::null_mut;

    use windows_sys::Win32::Security::{
        CopySid, GetLengthSid, GetTokenInformation, TOKEN_USER, TokenUser,
    };

    let mut required_size = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &raw mut required_size);
    }
    if required_size == 0 {
        return Err(io::Error::last_os_error());
    }

    let word_count = (required_size as usize).div_ceil(size_of::<u64>());
    let mut buffer = vec![0_u64; word_count];
    let success = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required_size,
            &raw mut required_size,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }

    let token_user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let sid_length = unsafe { GetLengthSid(token_user.User.Sid) };
    if sid_length == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut sid = vec![0_u8; sid_length as usize];
    if unsafe { CopySid(sid_length, sid.as_mut_ptr().cast(), token_user.User.Sid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sid)
}

impl Drop for MySqlOptionFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn serialize_option_file(target: &MySqlDsn) -> String {
    let mut contents = String::from("[client]\n");
    push_option(&mut contents, "host", &target.host);
    push_option(&mut contents, "port", &target.port.to_string());
    push_option(&mut contents, "user", &target.username);
    push_option(&mut contents, "password", &target.password);
    if let Some(database) = target.database.as_deref() {
        push_option(&mut contents, "database", database);
    }
    push_option(&mut contents, "ssl-mode", &target.ssl_mode.to_string());
    if let Some(path) = target.ssl_ca.as_deref() {
        push_option(&mut contents, "ssl-ca", path);
    }
    if let Some(path) = target.ssl_cert.as_deref() {
        push_option(&mut contents, "ssl-cert", path);
    }
    if let Some(path) = target.ssl_key.as_deref() {
        push_option(&mut contents, "ssl-key", path);
    }
    contents
}

fn push_option(contents: &mut String, key: &str, value: &str) {
    contents.push_str(key);
    contents.push_str(" = ");
    contents.push_str(&quote_option_value(value));
    contents.push('\n');
}

fn quote_option_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            _ => escaped.push(character),
        }
    }
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};

    use super::super::cli::test_support::MysqlProcess;
    use crate::domain::connection::MySqlSslMode;

    use super::*;

    fn target() -> MySqlDsn {
        MySqlDsn {
            host: "localhost".to_string(),
            port: 3306,
            database: None,
            username: "user".to_string(),
            password: "secret".to_string(),
            ssl_mode: MySqlSslMode::Disabled,
            ssl_ca: None,
            ssl_cert: None,
            ssl_key: None,
        }
    }

    #[test]
    fn option_file_quotes_syntax_characters_and_windows_paths() {
        let target = MySqlDsn {
            password: "p a#ss;=\"\\word".to_string(),
            database: Some("app".to_string()),
            ..target()
        };
        let contents = serialize_option_file(&target);

        assert!(contents.contains("password = \"p a#ss;=\\\"\\\\word\""));
        let mut certificate = String::new();
        push_option(&mut certificate, "ssl-ca", r"C:\certs\server.pem");
        assert_eq!(certificate, "ssl-ca = \"C:\\\\certs\\\\server.pem\"\n");
        assert_eq!(
            quote_option_value(r"C:\certs\server.pem"),
            r#""C:\\certs\\server.pem""#
        );
    }

    #[test]
    fn ipv6_host_serializes_without_url_brackets() {
        let target = MySqlDsn {
            host: "::1".to_string(),
            ..target()
        };

        assert!(serialize_option_file(&target).contains("host = \"::1\"\n"));
    }

    #[test]
    fn server_database_listing_option_file_omits_selected_database() {
        let mut target =
            super::super::dsn::parse_mysql_dsn("mysql://user:password@localhost:3306/app").unwrap();
        target.database = None;

        let contents = serialize_option_file(&target);

        assert!(!contents.contains("database ="));
    }

    #[test]
    fn option_file_serializes_tls_paths_without_option_syntax_confusion() {
        let ca_path = r#" C:\certs\ca #1;= "quoted".pem "#;
        let target = MySqlDsn {
            ssl_mode: MySqlSslMode::VerifyCa,
            ssl_ca: Some(ca_path.to_string()),
            ssl_cert: Some(r"C:\certs\client.pem".to_string()),
            ssl_key: Some(r"C:\certs\client-key.pem".to_string()),
            ..target()
        };

        let contents = serialize_option_file(&target);

        assert!(contents.contains("ssl-mode = \"VERIFY_CA\"\n"));
        assert!(contents.contains(&format!("ssl-ca = {}\n", quote_option_value(ca_path))));
        assert!(contents.contains("ssl-cert = \"C:\\\\certs\\\\client.pem\"\n"));
        assert!(contents.contains("ssl-key = \"C:\\\\certs\\\\client-key.pem\"\n"));
    }

    #[test]
    fn option_file_is_owner_only_and_removed_on_drop() {
        let option_file = MySqlOptionFile::create(&target()).unwrap();
        assert!(option_file.path.is_absolute());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&option_file.path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        #[cfg(windows)]
        assert_owner_only_acl(&option_file.path);
        let path = option_file.path.clone();
        drop(option_file);
        assert!(!path.exists());
    }

    #[cfg(windows)]
    fn assert_owner_only_acl(path: &std::path::Path) {
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;

        use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
        use windows_sys::Win32::Security::Authorization::GetNamedSecurityInfoW;
        use windows_sys::Win32::Security::{
            ACCESS_ALLOWED_ACE, AclSizeInformation, DACL_SECURITY_INFORMATION, EqualSid, GetAce,
            GetAclInformation, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
            OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
        };
        use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};

        let path = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut owner = null_mut();
        let mut dacl = null_mut();
        let mut security_descriptor: PSECURITY_DESCRIPTOR = null_mut();
        let status = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                windows_sys::Win32::Security::Authorization::SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &raw mut owner,
                null_mut(),
                &raw mut dacl,
                null_mut(),
                &raw mut security_descriptor,
            )
        };
        assert_eq!(status, ERROR_SUCCESS);

        let mut control = 0;
        let mut revision = 0;
        assert_ne!(
            unsafe {
                GetSecurityDescriptorControl(
                    security_descriptor,
                    &raw mut control,
                    &raw mut revision,
                )
            },
            0
        );
        assert_ne!(control & SE_DACL_PROTECTED, 0);

        let mut dacl_present = 0;
        let mut dacl_defaulted = 0;
        assert_ne!(
            unsafe {
                GetSecurityDescriptorDacl(
                    security_descriptor,
                    &raw mut dacl_present,
                    &raw mut dacl,
                    &raw mut dacl_defaulted,
                )
            },
            0
        );
        assert_ne!(dacl_present, 0);
        assert!(!dacl.is_null());

        let mut acl_info = windows_sys::Win32::Security::ACL_SIZE_INFORMATION::default();
        assert_ne!(
            unsafe {
                GetAclInformation(
                    dacl,
                    (&raw mut acl_info).cast::<std::ffi::c_void>(),
                    std::mem::size_of_val(&acl_info) as u32,
                    AclSizeInformation,
                )
            },
            0
        );
        assert_eq!(acl_info.AceCount, 1);

        let mut ace = null_mut();
        assert_ne!(unsafe { GetAce(dacl, 0, &raw mut ace) }, 0);
        let allowed_ace = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        assert_eq!(allowed_ace.Header.AceType, 0);
        assert_eq!(allowed_ace.Header.AceFlags, 0);
        assert_eq!(
            allowed_ace.Mask & (FILE_GENERIC_READ | FILE_GENERIC_WRITE),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE
        );
        let ace_sid = std::ptr::addr_of!(allowed_ace.SidStart).cast_mut().cast();
        assert_ne!(unsafe { EqualSid(owner, ace_sid) }, 0);

        unsafe {
            LocalFree(security_descriptor.cast());
        }
    }

    #[test]
    fn option_file_names_are_unique_uuid_v4_paths_under_concurrency() {
        let barrier = Arc::new(Barrier::new(16));
        let mut handles = Vec::with_capacity(16);
        for _ in 0..16 {
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                MySqlOptionFile::create(&target()).unwrap()
            }));
        }
        let files = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        let paths = files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let unique_paths = paths.iter().collect::<HashSet<_>>();

        assert_eq!(unique_paths.len(), paths.len());
        for path in &paths {
            let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap();
            let uuid = stem.strip_prefix("sabiql-mysql-").unwrap();
            assert_eq!(Uuid::parse_str(uuid).unwrap().get_version_num(), 4);
        }

        drop(files);
        assert!(paths.iter().all(|path| !path.exists()));
    }

    #[test]
    fn option_file_is_removed_when_mysql_process_start_fails() {
        let (result, path) = {
            let option_file = MySqlOptionFile::create(&target()).unwrap();
            let path = option_file.path.clone();
            let result = MysqlProcess::spawn_with_program(
                std::ffi::OsStr::new("__sabiql_missing_mysql_binary__"),
                &path,
            );
            (result, path)
        };

        assert!(result.is_err());
        assert!(!path.exists());
    }
}
