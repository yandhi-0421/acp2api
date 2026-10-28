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
    pub(super) fn assign(
        child: &tokio::process::Child,
        path: Option<&std::path::Path>,
    ) -> io::Result<Self> {
        unsafe {
            let name = path.map(job_name);
            let handle = CreateJobObjectW(
                ptr::null(),
                name.as_ref().map_or(ptr::null(), |name| name.as_ptr()),
            );
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
            resume(child.id().expect("new suspended process"))?;
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
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::{
        Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL},
    };
    private_path(path, "", |path, attributes| unsafe {
        let handle = CreateFileW(
            path,
            GENERIC_WRITE,
            0,
            attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(std::fs::File::from_raw_handle(handle.cast()))
        }
    })
}
pub(crate) fn private_folder(path: &std::path::Path) -> io::Result<()> {
    private_path(path, "OICI", |path, attributes| unsafe {
        if windows_sys::Win32::Storage::FileSystem::CreateDirectoryW(path, attributes) == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
}
fn private_path<T>(
    path: &std::path::Path,
    inheritance: &str,
    create: impl FnOnce(
        *const u16,
        *const windows_sys::Win32::Security::SECURITY_ATTRIBUTES,
    ) -> io::Result<T>,
) -> io::Result<T> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            },
            GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
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
        let descriptor: Vec<u16> = format!("O:{sid_text}D:P(A;{inheritance};FA;;;{sid_text})")
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
        let result = create(path.as_ptr(), &attributes);
        drop(allocation);
        result
    }
}

// A later start can check this job across Windows logon sessions.
fn job_name(path: &std::path::Path) -> Vec<u16> {
    use sha2::{Digest, Sha256};
    format!(
        "Global\\acp2api-{}",
        crate::hex(&Sha256::digest(path.as_os_str().as_encoded_bytes()))
    )
    .encode_utf16()
    .chain(Some(0))
    .collect()
}
pub(crate) fn process_tree_ended(path: &std::path::Path) -> bool {
    use windows_sys::Win32::System::SystemServices::JOB_OBJECT_QUERY;
    use windows_sys::Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
            OpenJobObjectW, QueryInformationJobObject,
        },
    };
    unsafe {
        let name = job_name(path);
        let handle = OpenJobObjectW(JOB_OBJECT_QUERY, 0, name.as_ptr());
        if handle.is_null() {
            return io::Error::last_os_error().raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32);
        }
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
        let ended = QueryInformationJobObject(
            handle,
            JobObjectBasicAccountingInformation,
            (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            std::mem::size_of_val(&info) as u32,
            ptr::null_mut(),
        ) != 0
            && info.ActiveProcesses == 0;
        CloseHandle(handle);
        ended
    }
}
fn resume(pid: u32) -> io::Result<()> {
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut found = Thread32First(snapshot, &mut entry);
        let mut result = Err(io::Error::other("suspended process has no initial thread"));
        while found != 0 {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                result = if thread.is_null() {
                    Err(io::Error::last_os_error())
                } else {
                    let resumed = ResumeThread(thread);
                    let error = io::Error::last_os_error();
                    CloseHandle(thread);
                    if resumed == u32::MAX {
                        Err(error)
                    } else {
                        Ok(())
                    }
                };
                break;
            }
            found = Thread32Next(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        result
    }
}

pub(crate) fn validate_private(path: &std::path::Path, protected: bool) -> io::Result<()> {
    use std::os::windows::{ffi::OsStrExt, fs::MetadataExt};
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            ACCESS_ALLOWED_ACE, ACL,
            Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
            DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetSecurityDescriptorControl,
            GetTokenInformation, OWNER_SECURITY_INFORMATION, SE_DACL_PROTECTED, TOKEN_QUERY,
            TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT,
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    if std::fs::symlink_metadata(path)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("reparse point"));
    }
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let mut owner = ptr::null_mut();
        let mut acl: *mut ACL = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        let code = GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        );
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let result = (|| {
            let mut token = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut size = 0;
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
            let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            let got = GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size,
                &mut size,
            );
            CloseHandle(token);
            if got == 0 {
                return Err(io::Error::last_os_error());
            }
            let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
            let mut control = 0;
            let mut revision = 0;
            if EqualSid(owner, user.User.Sid) == 0
                || acl.is_null()
                || GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) == 0
                || (protected && control & SE_DACL_PROTECTED == 0)
            {
                return Err(io::Error::other("nonprivate owner or ACL"));
            }
            for index in 0..u32::from((*acl).AceCount) {
                let mut ace = ptr::null_mut();
                if GetAce(acl, index, &mut ace) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let allowed = &*ace.cast::<ACCESS_ALLOWED_ACE>();
                if u32::from(allowed.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE
                    || EqualSid(
                        (&allowed.SidStart as *const u32).cast_mut().cast(),
                        user.User.Sid,
                    ) == 0
                {
                    return Err(io::Error::other("ACL grants another account access"));
                }
            }
            Ok(())
        })();
        LocalFree(descriptor);
        result
    }
}
