// Fixed seeds and finite bounds: 128 walks of 240 events, at most 400 settle rounds each.
use super::*;
// Safety fact 1: the count at the limit equals a shadow of reservations, releases, and resets.
// Safety fact 2: when all outstanding ACP calls end, no connected request stays queued with no way forward.
struct Walk {
    seed: u64,
    preparing: Vec<String>,
    running: Vec<String>,
    queued_or_working: Vec<String>,
    shadow: usize,
    next: usize,
    replacements: usize,
    sign_ins: usize,
    lockouts: usize,
    checks: usize,
    admissions:
        Vec<oneshot::Receiver<Result<crate::conversation::Ticket, crate::messages::Refusal>>>,
    connections: Vec<mpsc::UnboundedReceiver<()>>,
}

impl Walk {
    fn roll(&mut self, range: usize) -> usize {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed % range as u64) as usize
    }
    fn note(&mut self, f: &mut Fixture, commands: &[Command]) {
        for command in commands {
            match command {
                Command::Prepare { work, .. } => {
                    self.shadow += 1;
                    self.preparing.push(work.clone());
                }
                Command::Prompt { session, .. } | Command::Usage { session, .. } => {
                    self.running.push(session.clone());
                }
                Command::AdmitChecked { id, session } => {
                    let Work::Checking(request) = f.core.work.remove(id).unwrap() else {
                        panic!("a completed check owns its request")
                    };
                    let checked = f.core.check(&request.turn, 0);
                    let clock = crate::Ports::production().clock;
                    let accepted = f
                        .core
                        .accept(request, checked, &f.journal, clock.as_ref())
                        .map(Box::new);
                    let commands = f.event(Event::Checked {
                        id: *id,
                        accepted,
                        session: session.clone(),
                    });
                    self.note(f, &commands);
                    for admission in &mut self.admissions {
                        if let Ok(Ok(ticket)) = admission.try_recv() {
                            f.tickets.push(ticket.events);
                        }
                    }
                }
                Command::Lockout(_) => {
                    f.core.sessions.unload(|_| f.folder.join("walk-retained"));
                    f.core.replaced(None, None);
                    f.core.lifecycle.lockout_stopped();
                    self.shadow = 0;
                    self.preparing.clear();
                    self.running.clear();
                }
                _ => {}
            }
        }
    }
    fn check(&self, f: &Fixture, context: &str) {
        assert_eq!(
            f.core.sessions.full(),
            self.shadow >= 8,
            "{context}: shadow {} against full()",
            self.shadow
        );
    }
    fn replace_if_ready(&mut self, f: &mut Fixture) {
        if f.core.replacement_ready(0)
            && (f.core.lifecycle.replacement_pending()
                || matches!(f.core.lifecycle.state(), RuntimeState::Restarting))
        {
            let mut unused = Vec::new();
            f.core
                .lifecycle
                .refuse_build_changes(&crate::messages::Message::RuntimeFailed, &mut unused);
            self.replacements += 1;
            f.core.sessions.unload(|_| PathBuf::from("/probe"));
            f.core
                .replaced(Some(format!("startup-{}", self.replacements)), None);
            self.shadow = 1;
            let commands = f.event(Event::RuntimeReady {
                sign_in_required: false,
            });
            self.note(f, &commands);
            self.check(f, "after replacement");
        }
    }
    fn step(&mut self, f: &mut Fixture) {
        match self.roll(10) {
            0 | 1 if !f.core.queue_full() => {
                let text = format!("walk-{}-{}", self.seed, self.next);
                self.next += 1;
                let system = self.roll(4) != 0;
                let (work, commands) = f.request(&text, system);
                self.queued_or_working.push(work);
                self.note(f, &commands);
            }
            2 if !self.preparing.is_empty() => {
                let at = self.roll(self.preparing.len());
                let work = self.preparing.remove(at);
                let commands = if self.roll(4) == 0 {
                    self.shadow -= 1;
                    f.prepared(
                        &work,
                        Err(crate::messages::Message::Upstream("walk failure".into())),
                    )
                } else {
                    let session = format!("session-{}", self.next);
                    self.next += 1;
                    f.prepared(&work, Ok(&session))
                };
                self.note(f, &commands);
            }
            3 if !self.running.is_empty() => {
                let at = self.roll(self.running.len());
                let session = self.running.remove(at);
                f.tickets
                    .iter_mut()
                    .for_each(|ticket| while ticket.try_recv().is_ok() {});
                let commands = f.end_with_commands(&session);
                self.note(f, &commands);
            }
            4 if !self.queued_or_working.is_empty() => {
                let at = self.roll(self.queued_or_working.len());
                let work = self.queued_or_working.remove(at);
                // The actor sends Disconnected only for live work with a reply (conductor.rs:695).
                let live = f
                    .core
                    .work
                    .by_request(&work)
                    .is_some_and(|id| f.core.work[&id].reply().is_some());
                if live {
                    let commands = f.event(Event::Disconnected(work));
                    self.note(f, &commands);
                }
            }
            5 => {
                let (reply, _answer) = oneshot::channel();
                let commands = f.event(Event::ReadModelNames(reply));
                self.note(f, &commands);
            }
            6 => {
                if f.core.sign_in.wait().is_some() {
                    let commands = f.event(Event::EndSignInWait);
                    self.note(f, &commands);
                } else if matches!(
                    f.core.lifecycle.state(),
                    RuntimeState::Serving | RuntimeState::SessionLimitWait
                ) {
                    self.sign_ins += 1;
                    let commands = f.event(Event::StartSignIn);
                    self.note(f, &commands);
                }
            }
            7 if self.roll(8) == 0 => {
                self.lockouts += 1;
                let commands = f.event(Event::Lockout {
                    signal: "walk",
                    error: crate::messages::Message::Upstream("synthetic lockout".into()),
                    signalling_work: None,
                });
                self.note(f, &commands);
                let commands = f.event(Event::EndLockout);
                self.note(f, &commands);
            }
            8 if !f.core.queue_full() => {
                self.checks += 1;
                let text = format!("checked-{}-{}", self.seed, self.next);
                self.next += 1;
                let turn = crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"user","content":text}]})).unwrap()).ok().unwrap().turn;
                let request = f.journal.begin(crate::journal::Api::Chat, 0);
                self.queued_or_working.push(request.id.clone());
                let (admission, answer) = oneshot::channel();
                let (connection, connected) = mpsc::unbounded_channel();
                self.admissions.push(answer);
                self.connections.push(connected);
                let commands = f.event(Event::CheckPending(Box::new(PendingRequest {
                    mode: ResponseMode::Body,
                    turn,
                    request,
                    admission,
                    connection,
                })));
                self.note(f, &commands);
            }
            _ => {}
        }
        self.check(f, "after a step");
        self.replace_if_ready(f);
    }
    fn settle(&mut self, f: &mut Fixture) {
        if f.core.sign_in.wait().is_some() {
            let commands = f.event(Event::EndSignInWait);
            self.note(f, &commands);
        }
        for _ in 0..400 {
            while let Some(work) = self.preparing.pop() {
                let session = format!("session-{}", self.next);
                self.next += 1;
                let commands = f.prepared(&work, Ok(&session));
                self.note(f, &commands);
            }
            while let Some(session) = self.running.pop() {
                let commands = f.end_with_commands(&session);
                self.note(f, &commands);
            }
            self.check(f, "while settling");
            self.replace_if_ready(f);
            if self.preparing.is_empty() && self.running.is_empty() {
                break;
            }
        }
        assert_eq!(
            (f.core.work.places(), f.core.work.queued()),
            (0, 0),
            "a connected request stayed queued after all ACP calls ended; seed state waiting={} pending={}",
            matches!(f.core.lifecycle.state(), RuntimeState::SessionLimitWait),
            f.core.lifecycle.replacement_pending(),
        );
        assert!(!f.core.lifecycle.replacement_pending());
        assert!(matches!(f.core.lifecycle.state(), RuntimeState::Serving));
    }
}

impl Fixture {
    fn end_with_commands(&mut self, session: &str) -> Vec<Command> {
        let mut commands = self.event(Event::Runtime(RuntimeEvent::Turn {
            session: session.into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::EndTurn),
            },
        }));
        commands.extend(
            self.core
                .handle(Event::Deadline, 200, std::time::UNIX_EPOCH),
        );
        commands
    }
}

#[test]
fn fixed_seed_walk_preserves_session_capacity_and_queue_progress() {
    let (mut total_replacements, mut total_steps, mut sign_ins, mut lockouts, mut checks) =
        (0, 0, 0, 0, 0);
    for seed in 1..=128u64 {
        let mut f = Fixture::new();
        f.core.limit = 1 + (seed % 3) as u8;
        f.core.catalog = None;
        f.core.model_names.clear();
        let mut walk = Walk {
            seed: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            preparing: Vec::new(),
            running: Vec::new(),
            queued_or_working: Vec::new(),
            shadow: 7,
            next: 0,
            replacements: 0,
            sign_ins: 0,
            lockouts: 0,
            checks: 0,
            admissions: Vec::new(),
            connections: Vec::new(),
        };
        for _ in 0..240 {
            walk.step(&mut f);
            total_steps += 1;
        }
        walk.settle(&mut f);
        total_replacements += walk.replacements;
        sign_ins += walk.sign_ins;
        lockouts += walk.lockouts;
        checks += walk.checks;
    }
    println!(
        "walk: steps={total_steps} replacements={total_replacements} sign_ins={sign_ins} lockouts={lockouts} checks={checks}"
    );
    assert_eq!(total_steps, 30_720);
    assert!(sign_ins > 100 && lockouts > 100 && checks > 100);
    assert!(total_replacements > 100, "the walk must reach the limit");
}
