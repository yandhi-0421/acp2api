#[derive(Clone, Debug)]
pub(crate) struct ToolName(String);
impl ToolName {
    pub(crate) fn from_fields(name: Option<&str>, title: Option<&str>) -> Self {
        let candidate = name.or(title).unwrap_or("unknown");
        let candidate = candidate
            .strip_prefix("Run ")
            .and_then(|name| name.strip_suffix('?'))
            .unwrap_or(candidate);
        if candidate.is_empty()
            || candidate.len() > 128
            || !candidate
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        {
            return Self("unknown".into());
        }
        Self(candidate.into())
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
