use crate::{
    Moment,
    catalog::{Catalog, SelectedModel},
    conversation::{self, RecordHashes, ToolSetHash, TurnRequest},
    messages::Message,
};

pub(super) struct Wait {
    pub hashes: RecordHashes,
    pub tools: ToolSetHash,
    pub model: String,
    pub catalog: Option<Catalog>,
    pub deadline: Moment,
    pub error: Message,
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
                && model_id.as_deref() == Some(wait.model.as_str())
                && wait.tools == tools
                && wait.hashes.system == hashes.system
                && hashes.messages.starts_with(&wait.hashes.messages)
        })
        .map(|wait| Message::RetryWait {
            seconds: (wait.deadline - now).div_ceil(1000),
            error: Box::new(wait.error.clone()),
        })
}
