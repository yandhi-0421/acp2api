use argon2::{Argon2, PasswordHasher, password_hash::phc::PasswordHash};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
pub(crate) const STATE_FILE: &str = "state.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StateFile {
    format: u8,
    pub(crate) lockout: Option<Lockout>,
    pub(crate) checked_builds: Vec<CheckedBuild>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lockout {
    pub(crate) since: String,
    pub(crate) signal: String,
    pub(crate) original_error_text: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckedBuild {
    #[serde(flatten)]
    pub(crate) build: crate::ListedBuild,
    pub(crate) checked_at: String,
}
impl StateFile {
    pub(crate) fn read(path: &Path) -> std::io::Result<Self> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    format: 1,
                    lockout: None,
                    checked_builds: Vec::new(),
                });
            }
            Err(error) => {
                return Err(std::io::Error::new(
                    error.kind(),
                    format!("cannot read {}: {error}", path.display()),
                ));
            }
        };
        let state: Self = serde_json::from_slice(&bytes).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("cannot parse {}: {error}", path.display()),
            )
        })?;
        if state.format != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown state file format {}", state.format),
            ));
        }
        for checked in &state.checked_builds {
            if checked.build.version.is_empty()
                || ![
                    &checked.build.agy_acp_server_sha256,
                    &checked.build.localharness_external_sha256,
                ]
                .iter()
                .all(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "the state file has an invalid checked build",
                ));
            }
        }
        Ok(state)
    }
    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        atomic_write(path, &bytes)
    }
}
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
    pub needs_save: bool,
    pub new_key: Option<String>,
    pub new_password: Option<String>,
}
const SAVED_KEYS: [&str; 4] = [
    "port",
    "concurrency_limit",
    "local_api_key",
    "management_password_hash",
];
const OPTIONAL_KEY: &str = "agy_acp_server_path";
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
pub(crate) fn read(path: &Path, highest: u8) -> Result<Opened, String> {
    let text = match fs::read(path) {
        Ok(bytes) => Some(String::from_utf8(bytes).map_err(|e| {
            let offset = e.utf8_error().valid_up_to();
            invalid(
                path,
                &syntax_location(&String::from_utf8_lossy(e.as_bytes()), offset),
            )
        })?),
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
        if !SAVED_KEYS.contains(&key.as_str()) && key != OPTIONAL_KEY {
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
    let agy_acp_server_path = match table.get(OPTIONAL_KEY) {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .ok_or_else(|| invalid(path, OPTIONAL_KEY))?,
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
    Ok(Opened {
        settings,
        first_start: text.is_none(),
        needs_save: SAVED_KEYS.iter().any(|key| !table.contains_key(*key)),
        new_key,
        new_password,
    })
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
    let text = toml::to_string(settings).map_err(|_| "cannot encode configuration")?;
    atomic_write(path, text.as_bytes()).map_err(|error| error.to_string())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "stored file path has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}-{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        crate::random_hex::<16>().map_err(|error| std::io::Error::other(error.to_string()))?
    ));
    let result = (|| {
        let mut file = crate::os::private_file(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
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
#[cfg(test)]
mod tests {
    #[test]
    #[allow(clippy::disallowed_methods)]
    fn state_file_round_trips_records_and_rejects_invalid_data() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-state-file-{}",
            crate::random_hex::<12>().unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(super::STATE_FILE);
        let hash = "a".repeat(64);
        let saved = serde_json::json!({"format":1,"lockout":null,"checked_builds":[{
            "version":"1.6.0", "platform":"macos-arm64", "agy_acp_server_sha256":hash,
            "localharness_external_sha256":hash, "checked_at":"2026-10-18T12:00:00Z"
        }]});
        std::fs::write(&path, saved.to_string()).unwrap();
        let state = super::StateFile::read(&path).unwrap();
        state.save(&path).unwrap();
        let actual: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(actual, saved, "whole-file writes preserve the records");
        for invalid in [
            serde_json::json!({"format":2,"lockout":null,"checked_builds":[]}),
            serde_json::json!({"format":1,"lockout":null,"checked_builds":[],"extra":true}),
            {
                let mut value = saved.clone();
                value["checked_builds"][0]["unexpected"] = true.into();
                value
            },
            {
                let mut value = saved;
                value["checked_builds"][0]["agy_acp_server_sha256"] = "short".into();
                value
            },
        ] {
            std::fs::write(&path, invalid.to_string()).unwrap();
            assert!(
                super::StateFile::read(&path).is_err(),
                "the state parser must reject {invalid}"
            );
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            state.save(&path).is_err(),
            "a failed rename must return the filesystem error"
        );
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "a failed save leaves no temporary file"
        );
        assert!(
            super::StateFile::read(&path).is_err(),
            "an unreadable state file must return an error"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn non_utf8_configuration_gives_m26() {
        let folder = crate::os::environment()
            .temp
            .join(format!("acp2api-store-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("config.toml");
        for (bytes, place) in [
            (
                &b"port = 8329\nlocal_api_key = \"\xff\"\n"[..],
                "local_api_key",
            ),
            (&b"port = 8329\n\xff\n"[..], "at line 2"),
        ] {
            std::fs::write(&path, bytes).unwrap();
            let error = super::read(&path, 3).err().unwrap();
            assert_eq!(error, super::invalid(&path, place));
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
