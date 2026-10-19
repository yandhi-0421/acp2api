use agent_client_protocol_schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigSelectOption, SessionConfigSelectOptions,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::SystemTime};

#[derive(Serialize)]
pub(crate) struct ModelEntry {
    id: String,
    object: &'static str,
    r#type: &'static str,
    display_name: String,
    created: u64,
    created_at: String,
    owned_by: &'static str,
}
#[derive(Default)]
pub(crate) struct Catalog {
    models: Vec<OfferedModel>,
    created: u64,
    created_at: String,
}
struct OfferedModel {
    value: String,
    name: String,
    levels: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    config_options: Vec<SessionConfigOption>,
    models: Option<Models>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Models {
    available_models: Vec<AvailableModel>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AvailableModel {
    model_id: String,
}
impl Catalog {
    pub(crate) fn decode(value: &serde_json::Value, wall: SystemTime) -> Result<Self, String> {
        let metadata: Metadata = serde_json::from_value(value.clone())
            .map_err(|error| format!("ACP session/new model metadata: {error}"))?;
        let models = options(&metadata.config_options, "model")?
            .ok_or("ACP session/new has no model config option")?;
        let mut levels = options(&metadata.config_options, "thinking_level")?.unwrap_or_default();
        levels.sort_by_key(|level| match level.value.to_string().as_str() {
            "low" => 0,
            "medium" => 1,
            "high" => 2,
            _ => 3,
        });
        let available: HashSet<_> = metadata
            .models
            .into_iter()
            .flat_map(|m| m.available_models)
            .map(|m| m.model_id)
            .collect();
        let created = wall
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let created_at = crate::journal::timestamp(wall).map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        let bases: HashSet<_> = models.iter().map(|m| m.value.to_string()).collect();
        for model in models {
            let base = model.value.to_string();
            if base.is_empty() || model.name.is_empty() {
                return Err("ACP session/new has an empty model value or display name".into());
            }
            if seen.insert(base.clone()) {
                let mut seen_levels = HashSet::new();
                let supported = levels
                    .iter()
                    .map(|level| level.value.to_string())
                    .filter(|level| {
                        let id = format!("{base}-{level}");
                        available.contains(&id)
                            && !bases.contains(&id)
                            && seen_levels.insert(level.clone())
                    })
                    .collect();
                entries.push(OfferedModel {
                    value: base,
                    name: model.name.clone(),
                    levels: supported,
                });
            }
        }
        if entries.is_empty() {
            return Err("ACP session/new has no model values".into());
        }
        Ok(Self {
            models: entries,
            created,
            created_at,
        })
    }
    fn names(&self) -> impl Iterator<Item = (String, &str)> {
        self.models.iter().flat_map(|model| {
            std::iter::once(model.value.clone())
                .chain(
                    model
                        .levels
                        .iter()
                        .map(|level| format!("{}-{level}", model.value)),
                )
                .map(|id| (id, model.name.as_str()))
        })
    }
    pub(crate) fn entries(&self) -> Vec<ModelEntry> {
        self.names()
            .map(|(id, name)| ModelEntry {
                id,
                object: "model",
                r#type: "model",
                display_name: name.into(),
                created: self.created,
                created_at: self.created_at.clone(),
                owned_by: "google",
            })
            .collect()
    }
    pub(crate) fn check(&self, requested: &str) -> Result<(), crate::messages::Refusal> {
        if self.names().any(|(id, _)| id == requested) {
            Ok(())
        } else {
            Err(crate::messages::Message::Model {
                requested: requested.into(),
                names: self.names().map(|(id, _)| id).collect(),
            }
            .into())
        }
    }
}
fn options<'a>(
    config: &'a [SessionConfigOption],
    id: &str,
) -> Result<Option<Vec<&'a SessionConfigSelectOption>>, String> {
    let Some(option) = config.iter().find(|o| o.id.to_string() == id) else {
        return Ok(None);
    };
    match &option.kind {
        SessionConfigKind::Select(select) => Ok(Some(match &select.options {
            SessionConfigSelectOptions::Ungrouped(options) => options.iter().collect(),
            SessionConfigSelectOptions::Grouped(groups) => {
                groups.iter().flat_map(|g| &g.options).collect()
            }
            _ => {
                return Err(format!(
                    "ACP session/new config option {id} has unsupported select options"
                ));
            }
        })),
        _ => Err(format!("ACP session/new config option {id} is not select")),
    }
}
