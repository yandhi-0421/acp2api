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
#[derive(Default)]
pub(super) struct SignIn {
    required: bool,
    wait: Option<Wait>,
}
impl SignIn {
    pub(super) fn required(&self) -> bool {
        self.required
    }
    pub(super) fn clear_required(&mut self) {
        self.required = false;
    }
    pub(super) fn wait(&self) -> Option<&Wait> {
        self.wait.as_ref()
    }
    pub(super) fn start_wait(&mut self, now: Moment) {
        self.wait = Some(Wait::new(now));
    }
    pub(super) fn record_address(&mut self, address: String) {
        if let Some(wait) = &mut self.wait {
            wait.address = Some(address);
        }
    }
    /// Returns true when a wait was running.
    pub(super) fn end_wait(&mut self) -> bool {
        self.wait.take().is_some()
    }
}
#[cfg(test)]
impl SignIn {
    pub(super) fn set_required(&mut self) {
        self.required = true;
    }
}
impl Core {
    pub(super) fn sign_in_message(&self) -> Message {
        Message::SignInRequired(format!("{}#{}", self.panel_address, ENTRY_ID))
    }
    pub(super) fn require_sign_in(&mut self, commands: &mut Vec<Command>) {
        if !self.sign_in.required {
            self.sign_in.required = true;
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
