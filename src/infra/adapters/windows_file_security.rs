use std::fs::File;
use std::io;

pub(super) fn restrict_to_current_user(file: &File) -> io::Result<()> {
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

#[cfg(test)]
pub(super) mod test_support {
    use super::current_user_sid;

    pub(in crate::adapters) fn assert_owner_only_acl(path: &std::path::Path) {
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;

        use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, LocalFree};
        use windows_sys::Win32::Security::Authorization::GetNamedSecurityInfoW;
        use windows_sys::Win32::Security::{
            ACCESS_ALLOWED_ACE, AclSizeInformation, DACL_SECURITY_INFORMATION, EqualSid, GetAce,
            GetAclInformation, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
            PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED, TOKEN_QUERY,
        };
        use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let path = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut dacl = null_mut();
        let mut security_descriptor: PSECURITY_DESCRIPTOR = null_mut();
        let status = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                windows_sys::Win32::Security::Authorization::SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
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
        let mut token = null_mut();
        assert_ne!(
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) },
            0
        );
        let current_sid = current_user_sid(token).unwrap();
        unsafe {
            CloseHandle(token);
        }
        assert_ne!(
            unsafe { EqualSid(current_sid.as_ptr().cast_mut().cast(), ace_sid) },
            0
        );

        unsafe {
            LocalFree(security_descriptor.cast());
        }
    }
}
