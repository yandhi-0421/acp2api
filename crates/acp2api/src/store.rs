use argon2::{Argon2, PasswordHasher, password_hash::phc::PasswordHash};
use serde::Serialize;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize)]
pub(crate) struct Settings {
    pub port: u16,
    pub concurrency_limit: u8,
    pub local_api_key: String,
    pub management_password_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agy_acp_server_path: Option<PathBuf>,
}
pub(crate) struct Opened {
    pub settings: Settings,
    pub first_start: bool,
    pub new_key: Option<String>,
    pub new_password: Option<String>,
}
fn invalid(path: &Path, key: &str) -> String {
    format!(
        "the setting {key} in {} has a value that acp2api cannot use.",
        path.display()
    )
}
fn syntax_location(text: &str, offset: usize) -> String {
    let offset = offset.min(text.len());
    let line = text.as_bytes()[..offset]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1;
    let failed = text.lines().nth(line - 1).unwrap_or_default();
    if let Some((key, _)) = failed.split_once('=') {
        let key = key.trim();
        if !key.is_empty()
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return key.into();
        }
        if let Some(key) = key
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .filter(|s| !s.is_empty())
        {
            return key.into();
        }
    }
    format!("at line {line}")
}
pub(crate) fn read(
    path: &Path,
    highest: u8,
    deliver: impl FnOnce(&Opened) -> Result<(), String>,
) -> Result<Opened, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let table = match &text {
        Some(text) => toml::from_str::<toml::Table>(text).map_err(|e| {
            invalid(
                path,
                &syntax_location(text, e.span().map_or(0, |s| s.start)),
            )
        })?,
        None => toml::Table::new(),
    };
    for key in table.keys() {
        if !matches!(
            key.as_str(),
            "port"
                | "concurrency_limit"
                | "local_api_key"
                | "management_password_hash"
                | "agy_acp_server_path"
        ) {
            return Err(invalid(path, key));
        }
    }
    let port = match table.get("port") {
        None => 8329,
        Some(value) => value
            .as_integer()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|&n| n > 0)
            .ok_or_else(|| invalid(path, "port"))?,
    };
    let concurrency_limit = match table.get("concurrency_limit") {
        None => highest.min(2),
        Some(value) => value
            .as_integer()
            .and_then(|n| u8::try_from(n).ok())
            .filter(|n| (1..=highest).contains(n))
            .ok_or_else(|| invalid(path, "concurrency_limit"))?,
    };
    let agy_acp_server_path = match table.get("agy_acp_server_path") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .ok_or_else(|| invalid(path, "agy_acp_server_path"))?,
        ),
    };
    let mut new_key = None;
    let local_api_key = match table.get("local_api_key") {
        Some(value) => value
            .as_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| (0x21..=0x7e).contains(&b)))
            .ok_or_else(|| invalid(path, "local_api_key"))?
            .to_owned(),
        None => {
            let key = random_key()?;
            new_key = Some(key.clone());
            key
        }
    };
    let mut new_password = None;
    let management_password_hash = match table.get("management_password_hash") {
        Some(value) => {
            let hash = value
                .as_str()
                .ok_or_else(|| invalid(path, "management_password_hash"))?;
            if !valid_password_hash(hash) {
                return Err(invalid(path, "management_password_hash"));
            }
            hash.to_owned()
        }
        None => {
            let password = random_password()?;
            let hash = hash_password(&password)?;
            new_password = Some(password);
            hash
        }
    };
    let settings = Settings {
        port,
        concurrency_limit,
        local_api_key,
        management_password_hash,
        agy_acp_server_path,
    };
    let opened = Opened {
        settings,
        first_start: text.is_none(),
        new_key,
        new_password,
    };
    deliver(&opened)?;
    if text.is_none()
        || [
            "port",
            "concurrency_limit",
            "local_api_key",
            "management_password_hash",
        ]
        .iter()
        .any(|key| !table.contains_key(*key))
    {
        save(path, &opened.settings)?;
    }
    Ok(opened)
}
fn valid_password_hash(value: &str) -> bool {
    let Ok(hash) = PasswordHash::new(value) else {
        return false;
    };
    hash.algorithm.as_str() == "argon2id"
        && hash
            .version
            .is_some_and(|v| argon2::Version::try_from(v).is_ok())
        && hash.salt.as_ref().is_some_and(|salt| salt.len() >= 8)
        && hash.hash.as_ref().is_some_and(|output| output.len() >= 4)
        && ["m", "t", "p"]
            .iter()
            .all(|key| hash.params.get(*key).is_some())
        && argon2::Params::try_from(&hash).is_ok()
}
fn random_password() -> Result<String, String> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz23456789";
    let mut password = String::new();
    for i in 0..20 {
        if i > 0 && i % 5 == 0 {
            password.push('-');
        }
        loop {
            let mut byte = [0u8; 1];
            getrandom::fill(&mut byte).map_err(|_| "cannot generate management password")?;
            if byte[0] < 238 {
                password.push(ALPHABET[byte[0] as usize % ALPHABET.len()] as char);
                break;
            }
        }
    }
    Ok(password)
}
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::new();
    let mut bits = 0u32;
    let mut count = 0;
    for &byte in bytes {
        bits = (bits << 8) | u32::from(byte);
        count += 8;
        while count >= 6 {
            count -= 6;
            result.push(ALPHABET[((bits >> count) & 63) as usize] as char);
        }
    }
    if count > 0 {
        result.push(ALPHABET[((bits << (6 - count)) & 63) as usize] as char);
    }
    result
}
pub(crate) fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let parent = path.parent().ok_or("configuration path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut suffix = [0u8; 16];
    getrandom::fill(&mut suffix).map_err(|_| "cannot generate configuration temporary name")?;
    let temporary = parent.join(format!(".config-{}.tmp", base64url(&suffix)));
    let mut file = crate::os::private_file(&temporary).map_err(|e| e.to_string())?;
    let result = (|| {
        let text = toml::to_string(settings).map_err(|_| "cannot encode configuration")?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn random_key() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "cannot generate local API key")?;
    Ok(format!("acp2api-{}", base64url(&bytes)))
}
pub(crate) fn hash_password(password: &str) -> Result<String, String> {
    let params = argon2::Params::new(19456, 2, 1, Some(32))
        .map_err(|_| "cannot set password hash parameters")?;
    Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|_| "cannot hash management password".into())
}
pub(crate) fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::PasswordVerifier;
    PasswordHash::new(hash).ok().is_some_and(|h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}
