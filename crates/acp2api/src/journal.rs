use serde::Serialize;
use std::{io, path::Path, time::SystemTime};

#[derive(Clone, Copy, Debug, Serialize)]
pub enum Api {
    #[serde(rename = "chat.completions")]
    Chat,
    #[serde(rename = "messages")]
    Messages,
    #[serde(rename = "responses")]
    Responses,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionUse {
    New,
    Continue,
    Rebuild,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultKind {
    Success,
    Refused,
    Failed,
    Disconnected,
}
#[derive(Clone, Debug, Serialize)]
pub struct RequestRecord {
    pub time: String,
    pub request_id: String,
    pub api: Api,
    pub model: Option<String>,
    pub session: Option<String>,
    pub session_use: Option<SessionUse>,
    pub duration_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub result: ResultKind,
    pub error_code: Option<String>,
    pub unknown_fields: Vec<String>,
}
pub struct Writer;
impl Writer {
    pub fn open(_folder: &Path, _wall: SystemTime) -> io::Result<Self> {
        Ok(Self)
    }
    pub fn write(&mut self, _record: &RequestRecord) -> io::Result<usize> {
        Ok(0)
    }
}
