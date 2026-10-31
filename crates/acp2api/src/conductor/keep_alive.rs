use super::{Command, Core, work::ResponseChannel};
use crate::{Moment, conversation::TurnEvent, messages::Message};

pub(super) const KEEP_ALIVE_INTERVAL: Moment = 15_000;
pub(crate) const NO_UPDATE_LIMIT: Moment = 5 * 60 * 1000;

impl ResponseChannel {
    pub(super) fn emit(&mut self, event: TurnEvent, now: Moment, commands: &mut Vec<Command>) {
        if let Some(at) = &mut self.keep_alive {
            *at = now + KEEP_ALIVE_INTERVAL;
        }
        commands.push(Command::Emit(self.output.clone(), event));
    }
}

impl Core {
    pub(super) fn retire_idle(&mut self, now: Moment, commands: &mut Vec<Command>) {
        let idle: Vec<_> = self
            .work
            .iter()
            .filter_map(|(id, work)| work.update_deadline().filter(|at| *at <= now).map(|_| *id))
            .collect();
        for id in idle {
            let session = self.work[&id]
                .execution()
                .unwrap()
                .session()
                .unwrap()
                .to_owned();
            self.fail_request(id, session, Message::NoUpdate, now, commands);
        }
    }

    pub(super) fn emit_keep_alives(&mut self, now: Moment, commands: &mut Vec<Command>) {
        for work in self.work.values_mut() {
            if let Some(reply) = work.reply_mut()
                && reply.keep_alive.is_some_and(|at| at <= now)
            {
                reply.emit(TurnEvent::KeepAlive, now, commands);
            }
        }
    }
}
