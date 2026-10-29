use super::{Command, Core};
use crate::{Moment, messages::Message};

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
    pub(super) fn sign_in_message(&self) -> crate::messages::Message {
        crate::messages::Message::SignInRequired(self.sign_in_entry.clone())
    }
    pub(super) fn require_sign_in(&mut self, commands: &mut Vec<Command>) {
        if !self.sign_in_required {
            self.sign_in_required = true;
            self.refuse_queued(
                crate::journal::Outcome::Refused("sign_in_required"),
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
            self.refusal(now).unwrap()
        } else {
            error
        }
    }
}
