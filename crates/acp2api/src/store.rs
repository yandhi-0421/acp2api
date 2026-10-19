use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub port: u16,
    pub concurrency_limit: u8,
    pub local_api_key: String,
    pub management_password_hash: String,
    pub agy_acp_server_path: Option<std::path::PathBuf>,
}
pub(crate) fn read(path: &std::path::Path, highest: u8) -> Result<Settings, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let settings: Settings = toml::from_str(&text).map_err(|e| e.to_string())?;
    if settings.port == 0
        || !(1..=highest).contains(&settings.concurrency_limit)
        || settings.local_api_key.is_empty()
        || !settings.management_password_hash.starts_with("$argon2id$")
        || settings
            .agy_acp_server_path
            .as_ref()
            .is_some_and(|p| !p.is_absolute())
    {
        return Err("unusable configuration".into());
    }
    Ok(settings)
}
