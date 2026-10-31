use super::Failure;
use super::request::Active;
use crate::{
    Moment,
    catalog::Catalog,
    conversation::{self, EffortAsk, RecordHashes, ToolSetHash, TurnRequest},
    messages::Message,
};

pub(crate) const WAIT_MILLIS: Moment = 30_000;
pub(crate) const WAIT_SECONDS: u64 = WAIT_MILLIS / 1000;

pub(super) struct Wait {
    hashes: RecordHashes,
    tools: ToolSetHash,
    model_name: String,
    effort: EffortAsk,
    catalog: Option<Catalog>,
    pub(super) deadline: Moment,
    error: Message,
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
            hashes: active.record.hashes().clone(),
            tools: active.client_tools_hash,
            model_name: active.model_name.clone(),
            effort: active.effort.clone(),
            catalog: catalog.cloned(),
            deadline: failure.at.saturating_add(WAIT_MILLIS),
            error: failure.message.clone(),
        })
    }
}

pub(super) fn refusal(waits: &[Wait], turn: &TurnRequest, now: Moment) -> Option<Message> {
    let hashes = conversation::record_hashes(&turn.conversation);
    let tools = conversation::tool_set_hash(&turn.tools);
    waits
        .iter()
        .rev()
        .find(|wait| {
            wait.deadline > now
                && wait.tools == tools
                && wait.hashes.is_continued_by(&hashes)
                && wait.catalog.as_ref().map_or_else(
                    || wait.model_name == turn.model,
                    |catalog| {
                        catalog
                            .resolve(&wait.model_name, &wait.effort)
                            .ok()
                            .zip(catalog.resolve(&turn.model, &turn.effort).ok())
                            .is_some_and(|(earlier, later)| earlier.id() == later.id())
                    },
                )
        })
        .map(|wait| Message::RetryWait {
            seconds: (wait.deadline - now).div_ceil(1000),
            error: Box::new(wait.error.clone()),
        })
}
