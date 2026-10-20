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

#[derive(Default)]
pub(super) struct ToolCalls(std::collections::HashMap<(String, String), Observation>);
enum Observation {
    Pending(ToolName),
    PermissionAsked,
    Reported,
}
impl ToolCalls {
    pub(super) fn begin_prompt(&mut self, session: &str) {
        self.0.retain(|(owner, _), _| owner != session);
    }
    pub(super) fn permission(&mut self, session: &str, id: &str) {
        self.0
            .insert((session.into(), id.into()), Observation::PermissionAsked);
    }
    pub(super) fn update(
        &mut self,
        session: &str,
        id: &str,
        name: Option<&str>,
        title: Option<&str>,
        executing: bool,
    ) -> Option<ToolName> {
        let observation = self
            .0
            .entry((session.into(), id.into()))
            .or_insert_with(|| Observation::Pending(ToolName::from_fields(name, title)));
        match observation {
            Observation::Pending(previous) => {
                if name.is_some() || title.is_some() {
                    *previous = ToolName::from_fields(name, title);
                }
                if executing {
                    let name = previous.clone();
                    *observation = Observation::Reported;
                    Some(name)
                } else {
                    None
                }
            }
            Observation::PermissionAsked | Observation::Reported => None,
        }
    }
}
