use std::{io, path::PathBuf, ptr};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::{
        Com::CoTaskMemFree,
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
    },
    UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
};
pub(super) struct Job(usize);
impl Job {
    pub(super) fn assign(child: &tokio::process::Child) -> io::Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(ptr::null(), ptr::null());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(handle as usize);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let process = child
                .raw_handle()
                .ok_or_else(|| io::Error::other("child has no handle"))?;
            if AssignProcessToJobObject(handle, process as _) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0 as _);
        }
    }
}
pub(super) fn account_root() -> PathBuf {
    unsafe {
        let mut path = ptr::null_mut();
        let result = SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, ptr::null_mut(), &mut path);
        assert!(
            result >= 0 && !path.is_null(),
            "native account folder lookup failed"
        );
        let mut len = 0;
        while *path.add(len) != 0 {
            len += 1;
        }
        let folder = PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(
            path, len,
        )));
        CoTaskMemFree(path as _);
        folder.join("acp2api-account")
    }
}

pub(super) fn private_file(path: &std::path::Path) -> io::Result<std::fs::File> {
    use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
    use windows_sys::Win32::{
        Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree},
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            },
            GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    struct Local(*mut core::ffi::c_void);
    impl Drop for Local {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut size = 0;
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        let result = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        );
        CloseHandle(token);
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut sid = ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut sid) == 0 {
            return Err(io::Error::last_os_error());
        }
        let sid_allocation = Local(sid.cast());
        let mut len = 0;
        while *sid.add(len) != 0 {
            len += 1;
        }
        let sid_text = String::from_utf16_lossy(std::slice::from_raw_parts(sid, len));
        let descriptor: Vec<u16> = format!("O:{sid_text}D:P(A;;FA;;;{sid_text})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        drop(sid_allocation);
        let mut security = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor.as_ptr(),
            1,
            &mut security,
            ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let allocation = Local(security);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security,
            bInheritHandle: 0,
        };
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let handle = CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE,
            0,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        );
        let error = io::Error::last_os_error();
        drop(allocation);
        if handle == INVALID_HANDLE_VALUE {
            return Err(error);
        }
        Ok(std::fs::File::from_raw_handle(handle.cast()))
    }
}
