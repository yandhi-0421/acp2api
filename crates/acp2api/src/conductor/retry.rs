use super::{Active, Failure};
use crate::{
    Moment,
    catalog::{Catalog, SelectedModel},
    conversation::{self, RecordHashes, ToolSetHash, TurnRequest},
    messages::Message,
};

pub(crate) const WAIT_MILLIS: Moment = 30_000;

pub(super) struct Wait {
    pub hashes: RecordHashes,
    pub tools: ToolSetHash,
    pub model_id: String,
    pub catalog: Option<Catalog>,
    pub deadline: Moment,
    pub error: Message,
}
impl Wait {
    pub(super) fn after(
        active: &Active,
        failure: &Failure,
        catalog: Option<&Catalog>,
    ) -> Option<Self> {
        matches!(
            failure.message.row(),
            crate::messages::Row::Temporary503(_) | crate::messages::Row::Other502
        )
        .then(|| Self {
            hashes: active.hashes.clone(),
            tools: active.client_tools_hash,
            model_id: active.model_id.clone(),
            catalog: catalog.cloned(),
            deadline: failure.at.saturating_add(WAIT_MILLIS),
            error: failure.message.clone(),
        })
    }
}

pub(super) fn refusal(
    waits: &[Wait],
    turn: &TurnRequest,
    model: &SelectedModel,
    now: Moment,
) -> Option<Message> {
    let hashes = conversation::record_hashes(&turn.conversation);
    let tools = conversation::tool_set_hash(&turn.tools);
    waits
        .iter()
        .rev()
        .find(|wait| {
            let model_id = wait.catalog.as_ref().map_or_else(
                || Some(model.id()),
                |catalog| catalog.resolve(turn).ok().map(|model| model.id()),
            );
            wait.deadline > now
                && model_id.as_deref() == Some(wait.model_id.as_str())
                && wait.tools == tools
                && wait.hashes.is_continued_by(&hashes)
        })
        .map(|wait| Message::RetryWait {
            seconds: (wait.deadline - now).div_ceil(1000),
            error: Box::new(wait.error.clone()),
        })
}
