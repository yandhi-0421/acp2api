use super::{Command, Core};
use crate::{Moment, journal::Outcome, messages::Message};

pub(crate) const ENTRY_ID: &str = "sign-in";
const WAIT_DURATION: Moment = 600_000;

#[derive(Clone)]
pub(crate) struct Wait {
    pub(crate) deadline: Moment,
    pub(crate) address: Option<String>,
}
impl Wait {
    pub(super) fn new(now: Moment) -> Self {
        Self {
            deadline: now + WAIT_DURATION,
            address: None,
        }
    }
}
impl Core {
    pub(super) fn sign_in_message(&self) -> Message {
        Message::SignInRequired(format!("{}#{}", self.panel_address, ENTRY_ID))
    }
    pub(super) fn require_sign_in(&mut self, commands: &mut Vec<Command>) {
        if !self.sign_in_required {
            self.sign_in_required = true;
            self.refuse_queued(
                Outcome::Refused("sign_in_required"),
                self.sign_in_message(),
                commands,
            );
        }
    }
    pub(super) fn sign_in_error(
        &mut self,
        error: Message,
        now: Moment,
        commands: &mut Vec<Command>,
    ) -> Message {
        if error.requires_sign_in() {
            self.require_sign_in(commands);
            self.refusal(now).expect("sign-in required gives a refusal")
        } else {
            error
        }
    }
}
