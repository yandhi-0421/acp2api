use crate::conversation::{EffortAsk, ThinkingLevel};
use agent_client_protocol_schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigSelectOption, SessionConfigSelectOptions,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

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
#[derive(Clone)]
pub(crate) struct Catalog {
    models: Vec<OfferedModel>,
    created: u64,
    created_at: String,
}
#[derive(Clone)]
struct OfferedModel {
    value: String,
    name: String,
    levels: Vec<ThinkingLevel>,
}
impl OfferedModel {
    fn name_with_level(&self, level: ThinkingLevel) -> String {
        model_id(&self.value, level)
    }
}
pub(crate) struct SelectedModel {
    pub model: String,
    pub level: Option<ThinkingLevel>,
    pub level_from_name: bool,
}
impl SelectedModel {
    pub(crate) fn id(&self) -> String {
        match self.level {
            Some(level) => model_id(&self.model, level),
            None => self.model.clone(),
        }
    }
}
fn model_id(value: &str, level: ThinkingLevel) -> String {
    format!("{value}-{}", level.as_str())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    config_options: Vec<serde_json::Value>,
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
        let metadata = Metadata::deserialize(value)
            .map_err(|error| format!("ACP session/new model metadata: {error}"))?;
        let available: HashSet<_> = metadata
            .models
            .into_iter()
            .flat_map(|m| m.available_models)
            .map(|m| m.model_id)
            .collect();
        let models: Vec<_> = model_options(&metadata.config_options)?
            .into_iter()
            .map(|option| {
                let value = option.value.to_string();
                let mut model = OfferedModel {
                    value,
                    name: option.name,
                    levels: Vec::new(),
                };
                model.levels = [
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                ]
                .into_iter()
                .filter(|level| available.contains(&model.name_with_level(*level)))
                .collect();
                model
            })
            .collect();
        if models.is_empty() {
            return Err("ACP session/new has no model values".into());
        }
        Ok(Self {
            models,
            created_at: crate::journal::timestamp(wall).map_err(|e| e.to_string())?,
            created: wall.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        })
    }
    pub(crate) fn names(&self) -> impl Iterator<Item = (String, &str)> {
        self.models.iter().flat_map(|model| {
            std::iter::once(model.value.clone())
                .chain(
                    model
                        .levels
                        .iter()
                        .map(|level| model.name_with_level(*level)),
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
    pub(crate) fn resolve(
        &self,
        model_name: &str,
        effort: &EffortAsk,
    ) -> Result<SelectedModel, crate::messages::Refusal> {
        let found = self.models.iter().find_map(|model| {
            if model.value == model_name {
                Some((model, None))
            } else {
                model
                    .levels
                    .iter()
                    .copied()
                    .find(|level| model_name == model.name_with_level(*level))
                    .map(|level| (model, Some(level)))
            }
        });
        let (model, suffix) = found.ok_or_else(|| {
            crate::messages::Refusal::from(crate::messages::Message::Model {
                requested: model_name.into(),
                names: self.names().map(|(id, _)| id).collect(),
            })
        })?;
        let wanted = suffix.unwrap_or(match effort {
            EffortAsk::Level(level) => *level,
            EffortAsk::Absent | EffortAsk::Unknown(_) => ThinkingLevel::High,
        });
        let level = model
            .levels
            .iter()
            .find(|level| **level >= wanted)
            .or_else(|| model.levels.last())
            .copied();
        Ok(SelectedModel {
            model: model.value.clone(),
            level,
            level_from_name: suffix.is_some(),
        })
    }
}
fn model_options(config: &[serde_json::Value]) -> Result<Vec<SessionConfigSelectOption>, String> {
    let option = config
        .iter()
        .find(|o| o["id"] == "model")
        .ok_or("ACP session/new has no model config option")?;
    let option = SessionConfigOption::deserialize(option)
        .map_err(|error| format!("ACP session/new model config option: {error}"))?;
    match option.kind {
        SessionConfigKind::Select(select) => match select.options {
            SessionConfigSelectOptions::Ungrouped(options) => Ok(options),
            SessionConfigSelectOptions::Grouped(groups) => {
                Ok(groups.into_iter().flat_map(|g| g.options).collect())
            }
            _ => Err("ACP session/new model config option has unsupported select options".into()),
        },
        _ => Err("ACP session/new model config option is not select".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};
    fn captured_session_new() -> serde_json::Value {
        let capture = include_str!(
            "../../../docs/research/captures/2026-10-15-acp-probe/bad-model.transcript.ndjson"
        );
        let line: serde_json::Value =
            serde_json::from_str(capture.lines().nth(3).unwrap()).unwrap();
        line["msg"]["result"].clone()
    }
    fn ids(catalog: &Catalog) -> Vec<String> {
        catalog.names().map(|(id, _)| id).collect()
    }
    const WALL: Duration = Duration::from_secs(1_792_281_600);
    #[test]
    fn a_captured_session_new_gives_the_model_and_its_levels() {
        let catalog = Catalog::decode(&captured_session_new(), UNIX_EPOCH + WALL).unwrap();
        assert_eq!(
            ids(&catalog),
            [
                "gemini-3.8-flash",
                "gemini-3.8-flash-low",
                "gemini-3.8-flash-medium",
                "gemini-3.8-flash-high"
            ]
        );
    }
    #[test]
    fn an_unknown_config_option_type_does_not_stop_the_decode() {
        let mut result = captured_session_new();
        result["configOptions"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id":"later","name":"Later","type":"a_later_kind"}));
        let catalog = Catalog::decode(&result, UNIX_EPOCH + WALL).unwrap();
        assert_eq!(ids(&catalog).len(), 4);
    }
}
