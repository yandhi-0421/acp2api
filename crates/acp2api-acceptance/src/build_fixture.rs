use std::{fs, io, path::Path};
#[cfg(windows)]
pub const SERVER: &str = "agy_acp_server.exe";
#[cfg(not(windows))]
pub const SERVER: &str = "agy_acp_server.par";
#[cfg(windows)]
pub const HARNESS: &str = "localharness_external.exe";
#[cfg(not(windows))]
pub const HARNESS: &str = "localharness_external";

pub fn install(source: &Path, folder: &Path, version: &str, label: &str) -> io::Result<()> {
    fs::create_dir_all(folder)?;
    fs::copy(source.join(SERVER), folder.join(SERVER))?;
    let text = fs::read_to_string(source.join(HARNESS))?
        .replace("version = \"1.4.0\"", &format!("version = \"{version}\""))
        .replace("label = \"normal\"", &format!("label = \"{label}\""));
    fs::write(folder.join(HARNESS), text)
}
