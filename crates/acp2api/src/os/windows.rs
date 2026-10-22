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
