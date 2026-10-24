use crate::Environment;
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
};

pub(crate) fn root(environment: &Environment) -> Result<PathBuf, String> {
    let account = crate::hex(&Sha256::digest(
        environment.account_root.as_os_str().as_encoded_bytes(),
    ));
    let container = environment.temp.join("acp2api");
    shared_folder(&container)?;
    let account = container.join(account);
    shared_folder(&account)?;
    Ok(account)
}
fn shared_folder(path: &Path) -> Result<(), String> {
    match new_folder(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let meta = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
            if meta.is_dir() && !meta.is_symlink() {
                Ok(())
            } else {
                Err("the session temporary hierarchy contains a link or a non-directory".into())
            }
        }
        Err(error) => Err(error.to_string()),
    }
}
fn new_folder(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir(path)
    }
}
