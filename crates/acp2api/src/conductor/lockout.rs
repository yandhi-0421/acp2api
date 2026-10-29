use super::*;
use crate::{
    journal::Journal,
    messages::Message,
    store::{Lockout, StateFile},
};
use std::{path::Path, time::SystemTime};

pub(super) fn load(path: &Path, clock: &dyn Clock, journal: &Journal) -> Option<Lockout> {
    match StateFile::read(path) {
        Ok(state) => {
            // FR-035 requires a new normal state file after the owner deletes it.
            if !path.exists()
                && let Err(error) = state.save(path)
            {
                journal.state_file_failure("create", &error, clock);
            }
            state.lockout
        }
        Err(error) => Some(Lockout {
            since: crate::journal::timestamp(clock.wall()).expect("clock after the Unix epoch"),
            signal: "unreadable or damaged state file".into(),
            original_error_text: crate::journal::redact_error(&error),
        }),
    }
}

pub(super) fn persist(path: &Path, evidence: Lockout, journal: &Journal, clock: &dyn Clock) {
    let result = StateFile::read(path).and_then(|mut state| {
        state.lockout = Some(evidence);
        state.save(path)
    });
    if let Err(error) = result {
        journal.state_file_failure("lockout", &error, clock);
    }
}

impl Core {
    pub(super) fn upstream_error(
        &mut self,
        event: &mut Event,
        now: Moment,
        commands: &mut Vec<Command>,
    ) -> Option<Message> {
        let mut upstream_error = None;
        let mut failed_reply_signal = None;
        let error = match event {
            Event::Runtime(
                RuntimeEvent::Authenticated(Err(error))
                | RuntimeEvent::Prepared {
                    result: Err(error), ..
                }
                | RuntimeEvent::Turn {
                    update: TurnUpdate::Failed(error),
                    ..
                },
            ) => Some(error),
            Event::Runtime(RuntimeEvent::UpstreamError { session, error }) => {
                if let Some(active) = self
                    .by_session(session)
                    .and_then(|id| self.work[&id].execution())
                {
                    let reply = format!(
                        "{}{}{}",
                        active.text,
                        active.stop_prefix,
                        active.held.as_deref().unwrap_or("")
                    );
                    if reply.contains("Terms of Service") {
                        failed_reply_signal = Some("Terms of Service");
                    }
                }
                Some(upstream_error.insert(Message::UpstreamHttp(error.clone())))
            }
            _ => None,
        };
        if let Some(error) = error {
            if let Some(signal) = error.lockout_signal().or(failed_reply_signal)
                && self.lockout.is_none()
            {
                let error = error.clone();
                let signalling_work = match event {
                    Event::Runtime(RuntimeEvent::Prepared { work, .. }) => self.by_request(work),
                    Event::Runtime(
                        RuntimeEvent::Turn { session, .. }
                        | RuntimeEvent::UpstreamError { session, .. },
                    ) => self.by_session(session),
                    _ => None,
                };
                *event = Event::Lockout {
                    signal,
                    error,
                    signalling_work,
                };
            } else {
                *error = self.sign_in_error(error.clone(), now, commands);
            }
        }
        upstream_error
    }

    pub(super) fn start_lockout(
        &mut self,
        signal: &'static str,
        error: Message,
        signalling_work: Option<WorkId>,
        now: Moment,
        wall: SystemTime,
        commands: &mut Vec<Command>,
    ) {
        let evidence = Lockout {
            since: crate::journal::timestamp(wall).expect("clock after the Unix epoch"),
            signal: signal.into(),
            original_error_text: crate::journal::redact_error(&error.to_string()),
        };
        let message = Message::lockout(&evidence, &self.panel_address, false);
        let first_message = Message::lockout(&evidence, &self.panel_address, true);
        self.lockout = Some(evidence.clone());
        self.sign_in_wait = None;
        let before = commands.len();
        self.end_all_work(
            "lockout",
            |id| {
                if signalling_work == Some(id) {
                    first_message.clone()
                } else {
                    message.clone()
                }
            },
            now,
            commands,
        );
        // Persist and stop after cancellations, before any response can leave.
        let after_cancels = before
            + commands[before..]
                .iter()
                .take_while(|command| matches!(command, Command::Cancel(_)))
                .count();
        commands.insert(after_cancels, Command::Lockout(evidence));
        if let Some(replacement) = self.replacement.take() {
            match replacement {
                Replacement::Build { reply, .. } | Replacement::Check { reply, .. } => {
                    commands.push(Command::RefuseBuildChange {
                        reply,
                        error: message.clone(),
                    });
                }
                Replacement::SignIn => {}
            }
        }
        commands.extend(
            self.changes
                .drain(..)
                .map(|(_, reply)| Command::RefuseBuildChange {
                    reply,
                    error: message.clone(),
                }),
        );
    }

    pub(super) fn build_change(
        &mut self,
        action: crate::runtime_builds::Action,
        reply: oneshot::Sender<Result<String, LaunchFailure>>,
    ) {
        if let Some(evidence) = &self.lockout {
            let result = if matches!(action, crate::runtime_builds::Action::Initial) {
                Ok(String::new())
            } else {
                Err(LaunchFailure::Runtime(
                    Message::lockout(evidence, &self.panel_address, false).to_string(),
                ))
            };
            let _ = reply.send(result);
        } else {
            self.changes.push_back((action, reply));
        }
    }
}
