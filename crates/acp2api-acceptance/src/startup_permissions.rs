#[cfg(windows)]
pub fn assert_owner_only(path: &std::path::Path) {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::{
            ACCESS_ALLOWED_ACE,
            Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
            DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetTokenInformation,
            OWNER_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut owner = ptr::null_mut();
        let mut acl = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        assert_eq!(
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                &mut acl,
                ptr::null_mut(),
                &mut descriptor
            ),
            0,
            "read configuration security descriptor"
        );
        assert!(!acl.is_null(), "configuration must have an explicit DACL");
        assert_eq!(
            (*acl).AceCount,
            1,
            "configuration DACL must have only one account grant"
        );
        let mut token = ptr::null_mut();
        assert_ne!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token),
            0
        );
        let mut size = 0;
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        );
        CloseHandle(token);
        assert_ne!(ok, 0);
        let current = (&*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        assert_ne!(
            EqualSid(owner, current),
            0,
            "configuration owner must be current account"
        );
        let mut ace = ptr::null_mut();
        assert_ne!(GetAce(acl, 0, &mut ace), 0);
        let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
        assert_eq!(ace.Header.AceType, 0, "sole ACE must grant access");
        assert_eq!(
            ace.Header.AceFlags & 0x10,
            0,
            "configuration must not inherit a wider grant"
        );
        assert_ne!(
            EqualSid((&ace.SidStart as *const u32).cast_mut().cast(), current),
            0,
            "sole grant must name current account"
        );
        assert_ne!(ace.Mask & 0x0001, 0, "owner must have file read access");
        LocalFree(descriptor);
    }
}
