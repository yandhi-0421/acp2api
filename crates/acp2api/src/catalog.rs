use crate::conversation::{EffortAsk, ThinkingLevel, TurnRequest};
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
pub(crate) struct Catalog {
    models: Vec<OfferedModel>,
    created: u64,
    created_at: String,
}
struct OfferedModel {
    value: String,
    name: String,
    levels: Vec<ThinkingLevel>,
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
                let levels = [
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                ]
                .into_iter()
                .filter(|level| available.contains(&format!("{value}-{}", level.as_str())))
                .collect();
                OfferedModel {
                    value,
                    name: option.name,
                    levels,
                }
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
    fn names(&self) -> impl Iterator<Item = (String, &str)> {
        self.models.iter().flat_map(|model| {
            std::iter::once(model.value.clone())
                .chain(
                    model
                        .levels
                        .iter()
                        .map(|level| format!("{}-{}", model.value, level.as_str())),
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
        turn: &mut TurnRequest,
    ) -> Result<Option<serde_json::Value>, crate::messages::Refusal> {
        let found = self.models.iter().find_map(|model| {
            if model.value == turn.model {
                Some((model, None))
            } else {
                model
                    .levels
                    .iter()
                    .copied()
                    .find(|level| turn.model == format!("{}-{}", model.value, level.as_str()))
                    .map(|level| (model, Some(level)))
            }
        });
        let (model, suffix) = found.ok_or_else(|| {
            crate::messages::Refusal::from(crate::messages::Message::Model {
                requested: turn.model.clone(),
                names: self.names().map(|(id, _)| id).collect(),
            })
        })?;
        let mut unknown = None;
        let wanted = suffix.unwrap_or_else(|| match &turn.effort {
            EffortAsk::Level(level) => *level,
            EffortAsk::Absent => ThinkingLevel::High,
            EffortAsk::Unknown(value) => {
                unknown = Some(value.clone());
                ThinkingLevel::High
            }
        });
        turn.effort = match model
            .levels
            .iter()
            .find(|level| **level >= wanted)
            .or_else(|| model.levels.last())
        {
            Some(level) => EffortAsk::Level(*level),
            None => EffortAsk::Absent,
        };
        turn.model = model.value.clone();
        Ok(unknown)
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
