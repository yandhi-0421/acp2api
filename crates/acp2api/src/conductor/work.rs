use super::{
    ResponseMode,
    quota::{QuotaPhase, QuotaPurpose, QuotaQuery},
    records::OutstandingWork,
    request::Active,
};
use crate::{
    Hold, Moment,
    conversation::{Ticket, TurnEvent, TurnRequest},
    messages::OwnerMessage,
};
use std::collections::BTreeMap;
use tokio::sync::{mpsc, oneshot};

pub(super) struct PendingRequest {
    pub(super) mode: ResponseMode,
    pub(super) turn: TurnRequest,
    pub(super) request: crate::journal::Request,
    pub(super) admission: oneshot::Sender<Result<Ticket, crate::messages::Refusal>>,
    pub(super) connection: mpsc::UnboundedSender<()>,
}

pub(super) struct Accepted {
    pub(super) turn: TurnRequest,
    pub(super) model: crate::catalog::SelectedModel,
    pub(super) request: crate::journal::Request,
    pub(super) reply: ResponseChannel,
}

pub(super) type Output = mpsc::UnboundedSender<(TurnEvent, Hold)>;

pub(super) struct ResponseChannel {
    pub(super) output: Output,
    pub(super) keep_alive: Option<Moment>,
}

pub(super) type WorkId = u64;

#[derive(Default)]
pub(super) struct WorkTable {
    items: BTreeMap<WorkId, Work>,
    next: WorkId,
}
impl WorkTable {
    /// Adds work at the end of the queue.
    pub(super) fn add(&mut self, work: Work) {
        self.add_with(|_| work);
    }
    /// Adds work whose value holds its own id.
    pub(super) fn add_with(&mut self, work: impl FnOnce(WorkId) -> Work) {
        let id = self.next;
        self.next += 1;
        self.items.insert(id, work(id));
    }
    /// Puts work that `remove` took back under its id.
    pub(super) fn restore(&mut self, id: WorkId, work: Work) {
        self.items.insert(id, work);
    }
    pub(super) fn remove(&mut self, id: &WorkId) -> Option<Work> {
        self.items.remove(id)
    }
    pub(super) fn get_mut(&mut self, id: &WorkId) -> Option<&mut Work> {
        self.items.get_mut(id)
    }
    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut Work> {
        self.items.values_mut()
    }
    pub(super) fn places(&self) -> usize {
        self.items
            .values()
            .filter(|work| work.occupies_place())
            .count()
    }
    pub(super) fn queued(&self) -> usize {
        self.items.values().filter(|work| work.is_queued()).count()
    }
    pub(super) fn by_session(&self, session: &str) -> Option<WorkId> {
        self.items.iter().find_map(|(id, work)| {
            let current = match work {
                Work::CheckQueued(_)
                | Work::Checking(_)
                | Work::AcceptedQueued(_)
                | Work::QuotaQueued { .. }
                | Work::ModelsQueued(_)
                | Work::ModelsReading(_) => None,
                Work::AcceptedWorking(active) => active.session(),
                Work::Retiring(outstanding) => outstanding.session(),
                Work::QuotaRetiring { session, .. } => Some(session.as_str()),
                Work::QuotaWorking { phase, .. } => phase.session(),
            };
            (current == Some(session)).then_some(*id)
        })
    }
    pub(super) fn by_request(&self, request: &str) -> Option<WorkId> {
        self.items
            .iter()
            .find_map(|(id, work)| (work.key() == request).then_some(*id))
    }
}
impl std::ops::Deref for WorkTable {
    type Target = BTreeMap<WorkId, Work>;
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}
#[cfg(test)]
impl WorkTable {
    pub(super) fn clear(&mut self) {
        self.items.clear();
    }
}

pub(super) enum Work {
    ModelsQueued(ModelRead),
    ModelsReading(ModelRead),
    CheckQueued(PendingRequest),
    Checking(PendingRequest),
    AcceptedQueued(Accepted),
    AcceptedWorking(Active),
    Retiring(OutstandingWork),
    QuotaRetiring {
        work: String,
        session: String,
        failed_reply: Option<super::lockout::FailedReply>,
    },
    QuotaQueued {
        query: QuotaQuery,
        session: Option<String>,
    },
    QuotaWorking {
        failed_reply: Option<super::lockout::FailedReply>,
        query: QuotaQuery,
        phase: QuotaPhase,
        text: String,
        result: Result<(), crate::messages::Message>,
    },
}

pub(super) struct ModelRead {
    pub(super) work: String,
    pub(super) replies: Vec<oneshot::Sender<Result<(), OwnerMessage>>>,
}

impl Work {
    pub(super) fn failed_reply_mut(&mut self) -> Option<&mut Option<super::lockout::FailedReply>> {
        match self {
            Self::AcceptedWorking(active) => Some(&mut active.failed_reply),
            Self::Retiring(outstanding) => Some(&mut outstanding.failed_reply),
            Self::QuotaWorking { failed_reply, .. } => Some(failed_reply),
            Self::QuotaRetiring { failed_reply, .. } => Some(failed_reply),
            _ => None,
        }
    }

    pub(super) fn reply_text(&self) -> String {
        match self {
            Self::AcceptedWorking(active) => active.reply_text(),
            Self::Retiring(outstanding) => outstanding
                .failed_reply
                .as_ref()
                .map(|failed| failed.text().to_owned())
                .unwrap_or_default(),
            Self::QuotaWorking { text, .. } => text.clone(),
            Self::QuotaRetiring { failed_reply, .. } => failed_reply
                .as_ref()
                .map(|failed| failed.text().to_owned())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    pub(super) fn key(&self) -> &str {
        match self {
            Self::ModelsQueued(read) | Self::ModelsReading(read) => &read.work,
            Self::CheckQueued(request) | Self::Checking(request) => &request.request.id,
            Self::AcceptedQueued(request) => &request.request.id,
            Self::AcceptedWorking(active) => &active.request.id,
            Self::Retiring(outstanding) => outstanding.request_id(),
            Self::QuotaRetiring { work, .. } => work,
            Self::QuotaQueued { query, .. } | Self::QuotaWorking { query, .. } => &query.work,
        }
    }
    pub(super) fn execution(&self) -> Option<&Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_)
            | Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaRetiring { .. }
            | Self::QuotaQueued { .. }
            | Self::QuotaWorking { .. }
            | Self::ModelsQueued(_)
            | Self::ModelsReading(_) => None,
        }
    }
    pub(super) fn execution_mut(&mut self) -> Option<&mut Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_)
            | Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaRetiring { .. }
            | Self::QuotaQueued { .. }
            | Self::QuotaWorking { .. }
            | Self::ModelsQueued(_)
            | Self::ModelsReading(_) => None,
        }
    }
    pub(super) fn into_queued(self) -> Option<Accepted> {
        match self {
            Self::AcceptedQueued(request) => Some(request),
            Self::AcceptedWorking(_)
            | Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaRetiring { .. }
            | Self::QuotaQueued { .. }
            | Self::QuotaWorking { .. }
            | Self::ModelsQueued(_)
            | Self::ModelsReading(_) => None,
        }
    }
    pub(super) fn into_active(self) -> Option<Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_)
            | Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaRetiring { .. }
            | Self::QuotaQueued { .. }
            | Self::QuotaWorking { .. }
            | Self::ModelsQueued(_)
            | Self::ModelsReading(_) => None,
        }
    }
    pub(super) fn is_queued(&self) -> bool {
        matches!(
            self,
            Self::CheckQueued(_)
                | Self::AcceptedQueued(_)
                | Self::QuotaQueued { .. }
                | Self::ModelsQueued(_)
        )
    }
    pub(super) fn occupies_place(&self) -> bool {
        matches!(
            self,
            Self::Checking(_)
                | Self::AcceptedWorking(_)
                | Self::Retiring(_)
                | Self::QuotaRetiring { .. }
                | Self::QuotaWorking { .. }
                | Self::ModelsReading(_)
        )
    }
    pub(super) fn reply(&self) -> Option<&ResponseChannel> {
        match self {
            Self::AcceptedQueued(request) => Some(&request.reply),
            Self::AcceptedWorking(active) => Some(&active.reply),
            Self::QuotaQueued { query, .. } | Self::QuotaWorking { query, .. } => match &query
                .purpose
            {
                QuotaPurpose::Reset { response, .. } | QuotaPurpose::Refusal { response, .. } => {
                    response.as_ref().map(|active| &active.reply)
                }
                QuotaPurpose::Refresh(_) => None,
            },
            Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaRetiring { .. }
            | Self::ModelsQueued(_)
            | Self::ModelsReading(_) => None,
        }
    }
    pub(super) fn reply_mut(&mut self) -> Option<&mut ResponseChannel> {
        match self {
            Self::AcceptedQueued(request) => Some(&mut request.reply),
            Self::AcceptedWorking(active) => Some(&mut active.reply),
            Self::QuotaQueued { query, .. } | Self::QuotaWorking { query, .. } => match &mut query
                .purpose
            {
                QuotaPurpose::Reset { response, .. } | QuotaPurpose::Refusal { response, .. } => {
                    response.as_mut().map(|active| &mut active.reply)
                }
                QuotaPurpose::Refresh(_) => None,
            },
            Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaRetiring { .. }
            | Self::ModelsQueued(_)
            | Self::ModelsReading(_) => None,
        }
    }
    pub(super) fn update_deadline(&self) -> Option<Moment> {
        match self {
            Self::AcceptedWorking(active) => active.update_deadline(),
            _ => None,
        }
    }
    pub(super) fn deadline(&self) -> Option<Moment> {
        self.completion_deadline()
            .into_iter()
            .chain(self.update_deadline())
            .chain(self.reply().and_then(|reply| reply.keep_alive))
            .min()
    }
    pub(super) fn completion_deadline(&self) -> Option<Moment> {
        match self {
            Self::AcceptedWorking(active) => active.completion_deadline(),
            Self::QuotaWorking {
                phase: QuotaPhase::Settling { at, .. } | QuotaPhase::FailedPrompt { at, .. },
                ..
            } => Some(*at),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub(super) struct SessionSettings {
    pub(super) tools: Vec<crate::conversation::ClientTool>,
    pub(super) system: Option<String>,
}

impl SessionSettings {
    pub(super) fn for_turn(turn: &TurnRequest) -> Self {
        Self {
            system: crate::conversation::system_prompt_addition(&turn.conversation.system),
            tools: match turn.tool_choice {
                crate::conversation::ToolChoice::Off => Vec::new(),
                crate::conversation::ToolChoice::Auto => turn.tools.clone(),
            },
        }
    }
}
