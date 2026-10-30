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
                journal.state_file_failure(
                    "state_file_write_failed",
                    "create",
                    error.kind(),
                    clock,
                );
            }
            state.lockout
        }
        Err(error) => {
            let case = if error.kind() == std::io::ErrorKind::InvalidData {
                "damaged state file"
            } else {
                "state file cannot be read"
            };
            let diagnostic = format!("{case}: {:?}", error.kind());
            Some(Lockout {
                since: crate::journal::timestamp(clock.wall()).expect("clock after the Unix epoch"),
                signal: diagnostic.clone(),
                original_error_text: diagnostic,
            })
        }
    }
}

pub(super) fn persist(path: &Path, evidence: Lockout, journal: &Journal, clock: &dyn Clock) {
    match StateFile::read(path) {
        Ok(mut state) => {
            state.lockout = Some(evidence);
            if let Err(error) = state.save(path) {
                journal.state_file_failure(
                    "state_file_write_failed",
                    "lockout",
                    error.kind(),
                    clock,
                );
            }
        }
        Err(error) => {
            journal.state_file_failure("state_file_read_failed", "lockout", error.kind(), clock);
        }
    }
}

impl Core {
    pub(super) fn upstream_error(
        &mut self,
        event: &mut Event,
        now: Moment,
        commands: &mut Vec<Command>,
    ) -> Option<Message> {
        let Event::Runtime(runtime_event) = event else {
            return None;
        };
        let failed_reply = match runtime_event {
            RuntimeEvent::UpstreamError { session, .. } => self
                .by_session(session)
                .and_then(|id| self.work[&id].execution())
                .map(|active| {
                    format!(
                        "{}{}{}",
                        active.text,
                        active.stop_prefix,
                        active.held.as_deref().unwrap_or("")
                    )
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        let (error, signal) = runtime_event.upstream_error(&failed_reply)?;
        let mut upstream_error =
            matches!(runtime_event, RuntimeEvent::UpstreamError { .. }).then(|| error.clone());
        if let Some(signal) = signal
            && self.lockout.is_none()
        {
            let signalling_work = match runtime_event {
                RuntimeEvent::Prepared { work, .. } => self.by_request(work),
                RuntimeEvent::Turn { session, .. }
                | RuntimeEvent::UpstreamError { session, .. } => self.by_session(session),
                _ => None,
            };
            *event = Event::Lockout {
                signal,
                error,
                signalling_work,
            };
        } else {
            let error = self.sign_in_error(error, now, commands);
            if let Some(upstream) = &mut upstream_error {
                *upstream = error.clone();
            }
            match runtime_event {
                RuntimeEvent::Authenticated(Err(target))
                | RuntimeEvent::Prepared {
                    result: Err(target),
                    ..
                }
                | RuntimeEvent::Turn {
                    update: TurnUpdate::Failed(target),
                    ..
                } => *target = error,
                _ => {}
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
        reply: BuildReply,
    ) {
        if let Some(evidence) = &self.lockout {
            let result = if matches!(action, crate::runtime_builds::Action::Initial) {
                Ok(None)
            } else {
                Err(LaunchFailure::Owner(OwnerMessage::Api(Message::lockout(
                    evidence,
                    &self.panel_address,
                    false,
                ))))
            };
            let _ = reply.send(result);
        } else {
            self.changes.push_back((action, reply));
        }
    }
}
