use super::{Attempt, COMMAND_CAP, output, records, restore_options};
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use windows_sys::{
    Win32::Security::Authentication::Identity::{
        AUDIT_POLICY_INFORMATION, AuditFree, AuditQuerySystemPolicy, AuditSetSystemPolicy,
    },
    core::GUID,
};
const SUBCATEGORY: GUID = GUID::from_u128(0x0cce9226_69ae_11d9_bed3_505054503030);
const SUBCATEGORY_ARG: &str = "/subcategory:{0CCE9226-69AE-11D9-BED3-505054503030}";
const SUCCESS_AND_FAILURE: u32 = 3;

#[derive(Debug, Deserialize)]
struct Cursor {
    record: u64,
    time: String,
}

struct Audit {
    saved: Option<AUDIT_POLICY_INFORMATION>,
    rule: String,
    _lock: File,
}
impl std::fmt::Debug for Audit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audit").finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub(super) struct Capture {
    audit: Audit,
    cursor: Cursor,
}
impl Capture {
    pub fn begin() -> Result<Self, String> {
        let audit = Audit::begin()?;
        let cursor = serde_json::from_slice(&output(powershell("Cursor"))?.stdout)
            .map_err(|e| format!("Security cursor: {e}"))?;
        Ok(Self { audit, cursor })
    }
    pub fn command(&self, program: &Path) -> Command {
        Command::new(program)
    }
    pub fn read(&self, pids: &[u32]) -> Result<Vec<Attempt>, String> {
        if query()?.AuditingInformation & SUCCESS_AND_FAILURE != SUCCESS_AND_FAILURE {
            return Err("Filtering Platform Connection audit changed during observation".into());
        }
        let mut command = powershell("Read");
        command
            .arg("-Start")
            .arg(self.cursor.record.to_string())
            .arg("-Time")
            .arg(&self.cursor.time)
            .arg("-Pids")
            .arg(
                pids.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        records::windows(&output(command)?.stdout, self.cursor.record)
    }
    pub fn restore(&mut self) -> Result<(), String> {
        self.audit.restore()
    }
}
impl Audit {
    fn begin() -> Result<Self, String> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(std::env::temp_dir().join("acp2api-network-audit.lock"))
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + COMMAND_CAP;
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(format!("network audit lock: {error}")),
            }
        }
        use windows_sys::Win32::{
            Foundation::{CloseHandle, ERROR_SUCCESS, GetLastError, LUID},
            Security::{
                AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
                TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };
        let mut luid = LUID::default();
        let name: Vec<u16> = "SeSecurityPrivilege\0".encode_utf16().collect();
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut token = std::ptr::null_mut();
        if unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            ..Default::default()
        };
        privileges.Privileges[0].Luid = luid;
        privileges.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
        let enabled = unsafe {
            AdjustTokenPrivileges(
                token,
                0,
                &privileges,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        let error = unsafe { GetLastError() };
        unsafe { CloseHandle(token) };
        if enabled == 0 || error != ERROR_SUCCESS {
            return Err(format!("enable test-process SeSecurityPrivilege: {error}"));
        }
        let saved = query()?;
        let rule = format!(
            "acp2api-observer-{}-{}",
            std::process::id(),
            crate::setup::random_hex(6)
        );
        // Establish cleanup ownership before the first global policy change.
        let capture = Self {
            saved: Some(saved),
            rule,
            _lock: lock,
        };
        let mut auditpol = Command::new("auditpol.exe");
        auditpol.args([
            "/set",
            SUBCATEGORY_ARG,
            "/success:enable",
            "/failure:enable",
        ]);
        output(auditpol)?;
        if query()?.AuditingInformation & SUCCESS_AND_FAILURE != SUCCESS_AND_FAILURE {
            return Err("Filtering Platform Connection audit did not enable".into());
        }
        let mut firewall = powershell("BlockControl");
        firewall
            .arg("-Rule")
            .arg(&capture.rule)
            .arg("-Program")
            .arg(crate::setup::binary("network-observer-control"));
        output(firewall)?;
        Ok(capture)
    }
    pub fn restore(&mut self) -> Result<(), String> {
        if let Some(mut saved) = self.saved.take() {
            let mut command = powershell("UnblockControl");
            command.arg("-Rule").arg(&self.rule);
            let firewall = output(command);
            saved.AuditingInformation = restore_options(saved.AuditingInformation);
            let audit = unsafe { AuditSetSystemPolicy(&saved, 1) };
            if !audit {
                return Err(format!(
                    "restore audit policy: {}",
                    std::io::Error::last_os_error()
                ));
            }
            firewall?;
            if restore_options(query()?.AuditingInformation) != saved.AuditingInformation {
                return Err("restored audit policy differs from original".into());
            }
        }
        Ok(())
    }
}
impl Drop for Audit {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("network observer cleanup failed: {error}");
        }
    }
}
fn query() -> Result<AUDIT_POLICY_INFORMATION, String> {
    let mut policy = std::ptr::null_mut();
    if !unsafe { AuditQuerySystemPolicy(&SUBCATEGORY, 1, &mut policy) } {
        return Err(format!(
            "query audit policy: {}",
            std::io::Error::last_os_error()
        ));
    }
    let saved = unsafe { *policy };
    unsafe { AuditFree(policy.cast()) };
    Ok(saved)
}
fn powershell(operation: &str) -> Command {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/network_observer/windows.ps1"
        ),
        "-Operation",
        operation,
    ]);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires the elevated Windows CI runner and Security log access"]
    fn failed_run_restores_the_audit_policy_and_removes_the_control_rule() {
        let capture = Capture::begin().expect("native audit fixture");
        let original = capture.audit.saved.as_ref().unwrap().AuditingInformation;
        let rule = capture.audit.rule.clone();
        let failure = std::panic::catch_unwind(|| {
            let _capture = capture;
            panic!("planted run failure");
        });
        assert!(failure.is_err());
        assert_eq!(
            restore_options(query().unwrap().AuditingInformation),
            restore_options(original)
        );
        let mut check = Command::new("powershell.exe");
        check.args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!("$ErrorActionPreference='Stop'; try {{ Get-NetFirewallRule -Name '{rule}'; throw 'control rule survived failed run' }} catch {{ if ($_.FullyQualifiedErrorId -notlike 'CmdletizationQuery_NotFound_InstanceID,*') {{ throw }} }}"));
        output(check).expect("temporary firewall rule removed during unwind");
    }
}
