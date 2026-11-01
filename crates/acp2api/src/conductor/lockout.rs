use super::*;
use crate::{
    journal::Journal,
    messages::Message,
    store::{Lockout, StateFile},
};
use std::{path::Path, time::SystemTime};

#[derive(Clone)]
pub(super) struct FailedReply {
    error: Box<Message>,
    tail: String,
}

impl FailedReply {
    fn new(error: Message, text: &str) -> Self {
        let mut failed = Self {
            error: Box::new(error),
            tail: String::new(),
        };
        let _ = failed.observe(text);
        failed
    }

    fn observe(&mut self, text: &str) -> Option<&'static str> {
        self.tail.push_str(text);
        let signal = crate::messages::lockout_signal(None, &self.tail, None);
        // PERMISSION_DENIED is the longest lockout signal that can cross chunks.
        let mut start = self.tail.len().saturating_sub("PERMISSION_DENIED".len());
        while !self.tail.is_char_boundary(start) {
            start += 1;
        }
        self.tail.drain(..start);
        signal
    }

    pub(super) fn text(&self) -> &str {
        &self.tail
    }
}

pub(super) enum Loaded {
    Normal,
    Lockout(Lockout),
    NotWritable(std::io::ErrorKind),
}

pub(super) fn load(path: &Path, clock: &dyn Clock, journal: &Journal) -> Loaded {
    match StateFile::read(path) {
        Ok(state) => {
            if let Some(evidence) = state.lockout {
                Loaded::Lockout(evidence)
            } else if !path.exists() {
                // FR-035 requires a new normal state file after the owner deletes it.
                if let Err(error) = state.save(path) {
                    journal.state_file_failure(
                        "state_file_write_failed",
                        "create",
                        error.kind(),
                        clock,
                    );
                    Loaded::NotWritable(error.kind())
                } else {
                    Loaded::Normal
                }
            } else {
                Loaded::Normal
            }
        }
        Err(error) => {
            let case = if error.kind() == std::io::ErrorKind::InvalidData {
                "damaged state file"
            } else {
                "state file cannot be read"
            };
            let diagnostic = format!("{case}: {:?}", error.kind());
            Loaded::Lockout(Lockout {
                since: crate::journal::timestamp(clock.wall()).expect("clock after the Unix epoch"),
                signal: diagnostic.clone(),
                original_error_text: diagnostic,
            })
        }
    }
}

pub(super) struct HeldLockout {
    evidence: Lockout,
    unsaved: bool,
}

impl HeldLockout {
    pub(super) fn saved(evidence: Lockout) -> Self {
        Self {
            evidence,
            unsaved: false,
        }
    }

    pub(super) fn start(
        evidence: Lockout,
        path: &Path,
        journal: &Journal,
        clock: &dyn Clock,
    ) -> Self {
        let unsaved = write_failed(path, &evidence, journal, clock);
        Self { evidence, unsaved }
    }

    pub(super) fn evidence(&self) -> &Lockout {
        &self.evidence
    }

    pub(super) fn unsaved(&self) -> bool {
        self.unsaved
    }

    pub(super) fn retry_save(&mut self, path: &Path) -> bool {
        if !self.unsaved {
            return false;
        }
        if save_lockout(path, &self.evidence).is_ok() {
            self.unsaved = false;
            true
        } else {
            false
        }
    }
}

enum LockoutSaveError {
    Read(std::io::Error),
    Write(std::io::Error),
}

fn save_lockout(path: &Path, evidence: &Lockout) -> Result<(), LockoutSaveError> {
    let mut state = StateFile::read(path).map_err(LockoutSaveError::Read)?;
    state.lockout = Some(evidence.clone());
    state.save(path).map_err(LockoutSaveError::Write)
}

fn write_failed(path: &Path, evidence: &Lockout, journal: &Journal, clock: &dyn Clock) -> bool {
    match save_lockout(path, evidence) {
        Ok(()) => false,
        Err(LockoutSaveError::Read(error)) => {
            journal.state_file_failure("state_file_read_failed", "lockout", error.kind(), clock);
            error.kind() != std::io::ErrorKind::InvalidData
        }
        Err(LockoutSaveError::Write(error)) => {
            journal.state_file_failure("state_file_write_failed", "lockout", error.kind(), clock);
            true
        }
    }
}

pub(super) fn clear(path: &Path) -> std::io::Result<()> {
    // FR-035: the end must work also on a state file that keeps the lockout because it is damaged.
    let mut state = StateFile::read(path).unwrap_or_else(|_| StateFile::normal());
    state.lockout = None;
    state.save(path)
}

impl Core {
    pub(super) fn retry_lockout_save(&mut self) {
        if let Some(held) = &mut self.lockout
            && held.retry_save(&self.state_path)
        {
            self.publish_view();
        }
    }

    pub(super) fn end_lockout(&mut self, now: Moment) {
        self.lockout = None;
        self.lifecycle.lockout_ended(now);
    }

    pub(super) fn lockout_notice(&self) -> Option<String> {
        self.lockout
            .as_ref()
            .map(|held| Message::lockout(held.evidence(), &self.panel_address, false).to_string())
    }

    pub(super) fn lockout_unsaved(&self) -> bool {
        self.lockout.as_ref().is_some_and(|held| held.unsaved())
    }

    pub(super) fn held_lockout_start(
        &mut self,
        evidence: Lockout,
        journal: &Journal,
        clock: &dyn Clock,
    ) {
        self.lockout = Some(HeldLockout::start(
            evidence,
            &self.state_path,
            journal,
            clock,
        ));
    }

    pub(super) fn upstream_error(
        &mut self,
        event: &mut Event,
        now: Moment,
        commands: &mut Vec<Command>,
    ) -> Option<Message> {
        let Event::Runtime(runtime_event) = event else {
            return None;
        };
        let (error, signal) = match runtime_event {
            RuntimeEvent::UpstreamError { session, .. } => {
                let id = self.work.by_session(session);
                let text = id.map(|id| self.work[&id].reply_text()).unwrap_or_default();
                let result = runtime_event.upstream_error(&text)?;
                if let Some(slot) = id
                    .and_then(|id| self.work.get_mut(&id))
                    .and_then(Work::failed_reply_mut)
                {
                    *slot = Some(FailedReply::new(result.0.clone(), &text));
                }
                result
            }
            RuntimeEvent::Turn {
                session,
                update: TurnUpdate::Text(text),
            } => {
                let id = self.work.by_session(session)?;
                let failed = self.work.get_mut(&id)?.failed_reply_mut()?.as_mut()?;
                let signal = failed.observe(text)?;
                (failed.error.as_ref().clone(), Some(signal))
            }
            _ => runtime_event.upstream_error("")?,
        };
        let mut upstream_error =
            matches!(runtime_event, RuntimeEvent::UpstreamError { .. }).then(|| error.clone());
        if let Some(signal) = signal
            && self.lockout.is_none()
        {
            let signalling_work = match runtime_event {
                RuntimeEvent::Prepared { work, .. } => self.work.by_request(work),
                RuntimeEvent::Turn { session, .. }
                | RuntimeEvent::UpstreamError { session, .. } => self.work.by_session(session),
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
        self.lockout = Some(HeldLockout::saved(evidence.clone()));
        self.sign_in.end_wait();
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
        self.lifecycle.refuse_build_changes(&message, commands);
    }

    pub(super) fn build_input(
        &mut self,
        action: crate::runtime_builds::Action,
        reply: BuildReply,
        clock: &dyn Clock,
        journal: &Journal,
    ) -> Vec<Command> {
        let Some(reason) = self.lifecycle.state_file_reason().cloned() else {
            self.build_change(action, reply);
            return Vec::new();
        };
        if matches!(action, crate::runtime_builds::Action::Initial) {
            self.lifecycle.defer_start();
            let _ = reply.send(Ok(None));
            return Vec::new();
        }
        match load(&self.state_path, clock, journal) {
            Loaded::Normal => {
                self.lifecycle.state_file_made();
                self.lifecycle.queue(action, reply);
                Vec::new()
            }
            Loaded::NotWritable(_) => {
                let _ = reply.send(Err(LaunchFailure::Owner(OwnerMessage::Api(
                    Message::RuntimeUnavailable(reason),
                ))));
                Vec::new()
            }
            Loaded::Lockout(evidence) => {
                let _ = reply.send(Err(LaunchFailure::Owner(OwnerMessage::Api(
                    Message::lockout(&evidence, &self.panel_address, false),
                ))));
                self.handle(Event::StoredLockout(evidence), clock.now(), clock.wall())
            }
        }
    }
}
