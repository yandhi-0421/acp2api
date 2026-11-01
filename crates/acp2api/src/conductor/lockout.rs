use super::*;
use crate::{
    journal::Journal,
    messages::Message,
    store::{Lockout, StateFile},
};
use std::{path::Path, time::SystemTime};

pub(super) fn load(
    path: &Path,
    clock: &dyn Clock,
    journal: &Journal,
) -> (Option<Lockout>, Option<std::io::ErrorKind>) {
    match StateFile::read(path) {
        Ok(state) => {
            // FR-035 requires a new normal state file after the owner deletes it.
            let save_error = if !path.exists()
                && let Err(error) = state.save(path)
            {
                journal.state_file_failure(
                    "state_file_write_failed",
                    "create",
                    error.kind(),
                    clock,
                );
                Some(error.kind())
            } else {
                None
            };
            (state.lockout, save_error)
        }
        Err(error) => {
            let case = if error.kind() == std::io::ErrorKind::InvalidData {
                "damaged state file"
            } else {
                "state file cannot be read"
            };
            let diagnostic = format!("{case}: {:?}", error.kind());
            (
                Some(Lockout {
                    since: crate::journal::timestamp(clock.wall())
                        .expect("clock after the Unix epoch"),
                    signal: diagnostic.clone(),
                    original_error_text: diagnostic,
                }),
                None,
            )
        }
    }
}

pub(super) fn persist(
    path: &Path,
    evidence: Lockout,
    journal: &Journal,
    clock: &dyn Clock,
) -> bool {
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
                false
            } else {
                true
            }
        }
        Err(error) => {
            journal.state_file_failure("state_file_read_failed", "lockout", error.kind(), clock);
            false
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
        if !self.lockout_unsaved {
            return;
        }
        let Some(evidence) = self.lockout.clone() else {
            return;
        };
        let saved = StateFile::read(&self.state_path).and_then(|mut state| {
            state.lockout = Some(evidence);
            state.save(&self.state_path)
        });
        if saved.is_ok() {
            self.lockout_unsaved = false;
            self.publish_view();
        }
    }

    pub(super) fn end_lockout(&mut self, now: Moment) {
        self.lockout = None;
        self.lockout_unsaved = false;
        self.lifecycle.lockout_ended(now);
    }

    pub(super) fn lockout_notice(&self) -> Option<String> {
        self.lockout
            .as_ref()
            .map(|evidence| Message::lockout(evidence, &self.panel_address, false).to_string())
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
        let failed_reply = match runtime_event {
            RuntimeEvent::UpstreamError { session, .. } => self
                .work
                .by_session(session)
                .and_then(|id| self.work[&id].execution())
                .map(|active| active.reply_text())
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
        self.lockout = Some(evidence.clone());
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
}
