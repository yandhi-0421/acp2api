use std::{fs, path::PathBuf};

#[derive(Debug)]
pub struct DeniedCopy {
    path: PathBuf,
    #[cfg(unix)]
    original: fs::Permissions,
    #[cfg(windows)]
    original: Vec<u32>,
}

impl DeniedCopy {
    pub fn new(path: PathBuf) -> Self {
        #[cfg(unix)]
        let original = {
            use std::os::unix::fs::PermissionsExt;
            let original = fs::metadata(&path).unwrap().permissions();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o500)).unwrap();
            original
        };
        #[cfg(windows)]
        let original = deny_add_subdirectory(&path);
        let guard = Self { path, original };
        let probe = fs::create_dir(guard.path.join("copy-permission-probe")).unwrap_err();
        assert_eq!(probe.kind(), std::io::ErrorKind::PermissionDenied);
        guard
    }
}

impl Drop for DeniedCopy {
    fn drop(&mut self) {
        #[cfg(unix)]
        fs::set_permissions(&self.path, self.original.clone()).unwrap();
        #[cfg(windows)]
        restore_dacl(&self.path, &self.original);
    }
}

#[cfg(windows)]
fn deny_add_subdirectory(path: &std::path::Path) -> Vec<u32> {
    use acp2api_acceptance::startup_permissions::wide;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                DENY_ACCESS, EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, SE_FILE_OBJECT,
                SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_W,
            },
            DACL_SECURITY_INFORMATION, NO_INHERITANCE, OWNER_SECURITY_INFORMATION,
        },
        Storage::FileSystem::FILE_ADD_SUBDIRECTORY,
    };
    let mut owner = std::ptr::null_mut();
    let mut acl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let name = wide(path);
    let result = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut acl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(result, 0);
    assert!(!acl.is_null() && !owner.is_null());
    let acl_size = unsafe { (*acl).AclSize as usize };
    let mut original = vec![0u32; acl_size.div_ceil(4)];
    unsafe {
        std::ptr::copy_nonoverlapping(
            acl.cast::<u8>(),
            original.as_mut_ptr().cast::<u8>(),
            acl_size,
        );
    }
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ADD_SUBDIRECTORY,
        grfAccessMode: DENY_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            TrusteeForm: TRUSTEE_IS_SID,
            ptstrName: owner.cast(),
            ..Default::default()
        },
    };
    let mut denied = std::ptr::null_mut();
    let result = unsafe { SetEntriesInAclW(1, &entry, acl, &mut denied) };
    assert_eq!(result, 0);
    let result = unsafe {
        SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            denied,
            std::ptr::null_mut(),
        )
    };
    unsafe {
        LocalFree(denied.cast());
        LocalFree(descriptor);
    }
    assert_eq!(result, 0);
    original
}

#[cfg(windows)]
fn restore_dacl(path: &std::path::Path, original: &[u32]) {
    use acp2api_acceptance::startup_permissions::wide;
    use windows_sys::Win32::Security::{
        Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW},
        DACL_SECURITY_INFORMATION,
    };
    let name = wide(path);
    let result = unsafe {
        SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            original.as_ptr().cast(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(result, 0);
}
