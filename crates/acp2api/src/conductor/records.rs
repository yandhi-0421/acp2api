use super::request::{Active, Exit};
use super::work::SessionSettings;
use super::{Command, PendingRequest};
use crate::Moment;
use crate::conversation::{self, Item, Prompt, RecordHashes, ToolSetHash};
use std::collections::BTreeMap;
pub(super) enum Location {
    Loaded,
    Unloaded(std::path::PathBuf),
}
pub(super) struct Record {
    pub location: Location,
    pub session: String,
    pub record: RequestRecord,
}
pub(super) struct OutstandingWork {
    pub(super) reply_signals: super::lockout::ReplySignals,
    model: String,
    request: crate::journal::Request,
    session: Option<String>,
    completion: PendingCompletion,
}
pub(super) enum PendingCompletion {
    Disconnected(Option<crate::journal::SessionUse>),
    AlreadyCompleted,
}
impl OutstandingWork {
    pub(super) fn from_active(active: &Active, completion: PendingCompletion) -> Self {
        Self {
            reply_signals: active.reply_signals.clone(),
            model: active.model.clone(),
            request: active.request.clone(),
            session: active.session().map(str::to_owned),
            completion,
        }
    }
    pub(super) fn disconnected_check(request: PendingRequest) -> Self {
        Self {
            reply_signals: super::lockout::ReplySignals::default(),
            model: request.turn.model,
            request: request.request,
            session: None,
            completion: PendingCompletion::Disconnected(None),
        }
    }
    pub(super) fn request_id(&self) -> &str {
        &self.request.id
    }
    pub(super) fn model(&self) -> &str {
        &self.model
    }
    pub(super) fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }
    pub(super) fn prepared(&mut self, session: String) {
        self.session = Some(session);
    }
    pub(super) fn finish(self) -> Vec<Command> {
        let mut commands = Vec::new();
        if let Some(session) = &self.session {
            commands.push(Command::Forget(session.clone()));
        }
        if let PendingCompletion::Disconnected(kind) = self.completion {
            commands.push(Command::Complete(crate::journal::Completion {
                request: self.request,
                session: self.session,
                session_use: kind,
                outcome: crate::journal::Outcome::Disconnected,
            }));
        }
        commands
    }
}
pub(super) struct RequestRecord {
    hashes: RecordHashes,
    last_reply: LastReply,
    tools: ToolSetHash,
    used: u64,
    expires: Moment,
}
#[derive(Default)]
struct LastReply {
    text: Option<conversation::ItemHash>,
    tool_call: Option<(String, String)>,
}
impl LastReply {
    fn len(&self) -> usize {
        usize::from(self.text.is_some()) + usize::from(self.tool_call.is_some())
    }
    fn matches(&self, mut items: &[&Item]) -> bool {
        if let Some(hash) = self.text {
            match items {
                [
                    Item::Message {
                        role: conversation::Role::Assistant,
                        name: None,
                        parts,
                    },
                    rest @ ..,
                ] if matches!(&parts[..], [conversation::Part::Text(text)]
                    if conversation::clean_reply_hash(text) == hash) =>
                {
                    items = rest
                }
                _ => return false,
            }
        }
        match &self.tool_call {
            Some((id, name)) => matches!(items.first(),
                Some(Item::ToolCall { id: call_id, name: call_name, .. })
                    if call_id == id && call_name == name),
            None => true,
        }
    }
}
impl RequestRecord {
    pub(super) fn with_reply(mut self, text: &str, tool_call: Option<(String, String)>) -> Self {
        self.last_reply = LastReply {
            text: (tool_call.is_none() || !text.is_empty())
                .then(|| conversation::clean_reply_hash(text)),
            tool_call,
        };
        self
    }
    fn len(&self) -> usize {
        self.hashes.messages.len() + self.last_reply.len()
    }
    pub(super) fn hashes(&self) -> &RecordHashes {
        &self.hashes
    }
    #[cfg(test)]
    pub(super) fn tools(&self) -> ToolSetHash {
        self.tools
    }
    #[cfg(test)]
    pub(super) fn fixture(
        hashes: RecordHashes,
        tools: ToolSetHash,
        used: u64,
        expires: Moment,
    ) -> Self {
        Self {
            hashes,
            last_reply: LastReply::default(),
            tools,
            used,
            expires,
        }
    }
}
pub(super) struct Selection {
    pub(super) kind: crate::journal::SessionUse,
    pub(super) prompt: Vec<crate::conversation::PromptBlock>,
    pub(super) ready: Option<(String, Location)>,
    pub(super) record: RequestRecord,
}
pub(super) const SESSIONS_PER_RUNTIME_PROCESS: usize = 8;

#[derive(Clone, Copy)]
enum Source {
    Record(usize),
    Checked,
    Startup,
    New,
}

#[derive(Default)]
pub(super) struct AcpSessions {
    // Successful session/new and session/load calls plus pending preparations.
    // Forgetting a record does not close its ACP session in the runtime process.
    held: usize,
    records: Vec<Record>,
    startup: Option<String>,
    use_sequence: u64,
    cancels: BTreeMap<String, Moment>,
    settled_replies: BTreeMap<String, (super::lockout::ReplySignals, Moment)>,
}
impl AcpSessions {
    pub(super) fn remember_reply(
        &mut self,
        session: String,
        reply: super::lockout::ReplySignals,
        until: Moment,
    ) {
        self.settled_replies.insert(session, (reply, until));
    }
    pub(super) fn reply_signals_mut(
        &mut self,
        session: &str,
        now: Moment,
    ) -> Option<&mut super::lockout::ReplySignals> {
        self.settled_replies
            .get_mut(session)
            .filter(|(_, until)| *until >= now)
            .map(|(reply, _)| reply)
    }
    #[cfg(test)]
    pub(super) fn records(&self) -> &[Record] {
        &self.records
    }
    pub(super) fn retained(&self) -> Vec<(String, std::path::PathBuf)> {
        self.records
            .iter()
            .filter_map(|record| match &record.location {
                Location::Unloaded(cwd) => Some((record.session.clone(), cwd.clone())),
                Location::Loaded => None,
            })
            .collect()
    }
    pub(super) fn unload(&mut self, mut cwd: impl FnMut(&str) -> std::path::PathBuf) {
        for record in &mut self.records {
            if matches!(record.location, Location::Loaded) {
                record.location = Location::Unloaded(cwd(&record.session));
            }
        }
    }
    pub(super) fn replaced(&mut self, startup: Option<String>) {
        self.settled_replies.clear();
        self.held = usize::from(startup.is_some());
        self.startup = startup;
    }
    pub(super) fn prepare(
        &mut self,
        work: String,
        settings: SessionSettings,
        load: Option<(String, std::path::PathBuf)>,
    ) -> Command {
        self.held += 1;
        Command::Prepare {
            work,
            settings,
            load,
        }
    }
    pub(super) fn preparation_failed(&mut self) {
        self.held -= 1;
    }
    pub(super) fn full(&self) -> bool {
        self.held >= SESSIONS_PER_RUNTIME_PROCESS
    }
    pub(super) fn at_capacity(
        &self,
        turn: &crate::conversation::TurnRequest,
        settings: &SessionSettings,
        checked: bool,
    ) -> bool {
        if !self.full() {
            return false;
        }
        let hashes = crate::conversation::record_hashes(&turn.conversation);
        let tools_hash = crate::conversation::tool_set_hash(&settings.tools);
        let items = turn.conversation.items();
        match self.source(&hashes, tools_hash, &items, settings, checked) {
            Source::Record(index) => matches!(self.records[index].location, Location::Unloaded(_)),
            Source::New => true,
            Source::Checked | Source::Startup => false,
        }
    }
    fn source(
        &self,
        hashes: &RecordHashes,
        tools: ToolSetHash,
        items: &[&Item],
        settings: &SessionSettings,
        checked: bool,
    ) -> Source {
        if let Some((index, _)) = select(&self.records, hashes, tools, items) {
            Source::Record(index)
        } else if checked {
            Source::Checked
        } else if settings.system.is_none() && settings.tools.is_empty() && self.startup.is_some() {
            Source::Startup
        } else {
            Source::New
        }
    }
    pub(super) fn models_ready(&mut self, session: String) -> Option<Command> {
        self.startup.replace(session).map(Command::Forget)
    }
    pub(super) fn take_startup(&mut self) -> Option<String> {
        self.startup.take()
    }
    #[cfg(test)]
    pub(super) fn remember(&mut self, record: Record) {
        self.records.push(record);
    }
    pub(super) fn forget(&mut self, session: String) -> Command {
        self.records.retain(|record| record.session != session);
        if self.startup.as_ref() == Some(&session) {
            self.startup = None;
        }
        Command::Forget(session)
    }
    pub(super) fn forget_idle(&mut self) -> Vec<Command> {
        let idle = self.records.drain(..).map(|record| record.session);
        idle.chain(self.startup.take())
            .map(Command::Forget)
            .collect()
    }
    pub(super) fn expire(&mut self, now: crate::Moment) -> Vec<Command> {
        self.settled_replies.retain(|_, (_, until)| *until >= now);
        let mut commands = Vec::new();
        self.records.retain(|record| {
            if record.record.expires <= now {
                commands.push(Command::Forget(record.session.clone()));
                false
            } else {
                true
            }
        });
        commands
    }
    pub(super) fn select(
        &mut self,
        turn: &crate::conversation::TurnRequest,
        settings: &SessionSettings,
        mut checked_session: Option<String>,
        now: Moment,
    ) -> (Selection, Vec<Command>) {
        let mut commands = Vec::new();
        let hashes = crate::conversation::record_hashes(&turn.conversation);
        let tools_hash = crate::conversation::tool_set_hash(&settings.tools);
        let items = turn.conversation.items();
        let source = self.source(
            &hashes,
            tools_hash,
            &items,
            settings,
            checked_session.is_some(),
        );
        let matched = match source {
            Source::Record(index) => Some((index, &items[self.records[index].record.len()..])),
            _ => None,
        };
        let prompt = match matched {
            Some((_, new_items)) => Prompt::Continue { new_items },
            None if turn.conversation.earlier.is_empty() => Prompt::New {
                message: &turn.conversation.last,
            },
            None => Prompt::Rebuild {
                conversation_items: &items,
            },
        };
        let (kind, prompt) = crate::conversation::prompt(prompt, &items);
        let ready = match source {
            Source::Record(index) => {
                let record = self.records.remove(index);
                Some((record.session, record.location))
            }
            Source::Checked => checked_session
                .take()
                .map(|session| (session, Location::Loaded)),
            Source::Startup => self.startup.take().map(|id| (id, Location::Loaded)),
            Source::New => None,
        };
        if matched.is_some() {
            commands.extend(checked_session.map(Command::Forget));
        }
        self.use_sequence += 1;
        let record = RequestRecord {
            hashes,
            last_reply: LastReply::default(),
            tools: tools_hash,
            used: self.use_sequence,
            expires: now.saturating_add(24 * 60 * 60 * 1000),
        };
        (
            Selection {
                kind,
                prompt,
                ready,
                record,
            },
            commands,
        )
    }
    pub(super) fn next_deadline(&self) -> Option<Moment> {
        self.records
            .iter()
            .map(|record| record.record.expires)
            .chain(self.cancels.values().copied())
            .min()
    }
    pub(super) fn cancellation_due(&self, now: Moment) -> bool {
        self.cancels.values().any(|at| *at <= now)
    }
    pub(super) fn cancel(&mut self, session: String, now: Moment) -> Command {
        self.cancels
            .entry(session.clone())
            .or_insert_with(|| now.saturating_add(super::CANCEL_TIMEOUT));
        Command::Cancel(session)
    }
    pub(super) fn cancel_ended(&mut self, session: &str) {
        self.cancels.remove(session);
    }
    pub(super) fn clear_cancels(&mut self) {
        self.cancels.clear();
    }
    pub(super) fn runtime_failed(&mut self) {
        self.settled_replies.clear();
        self.clear_cancels();
        self.startup = None;
    }
    pub(super) fn disconnect(
        &mut self,
        active: Active,
        now: Moment,
    ) -> (Option<OutstandingWork>, Vec<Command>) {
        let mut commands = Vec::new();
        let exit = active.exit();
        if matches!(exit, Exit::Settled)
            && let Some(session) = active.session()
        {
            self.remember_reply(
                session.to_owned(),
                active.reply_signals.clone(),
                active.completion_deadline().unwrap(),
            );
        }
        if matches!(exit, Exit::CancelAndRetire) {
            commands.push(self.cancel(active.session().unwrap().to_owned(), now));
        }
        let outstanding = OutstandingWork::from_active(
            &active,
            PendingCompletion::Disconnected(Some(active.kind)),
        );
        if matches!(exit, Exit::Settled) {
            commands.extend(outstanding.finish());
            (None, commands)
        } else {
            (Some(outstanding), commands)
        }
    }
    pub(super) fn keep(
        &mut self,
        session: String,
        record: Option<RequestRecord>,
    ) -> Option<String> {
        match record {
            Some(record) => {
                self.records.push(Record {
                    location: Location::Loaded,
                    session,
                    record,
                });
                None
            }
            None => Some(session),
        }
    }
    #[cfg(test)]
    pub(super) fn fixture(
        records: Vec<Record>,
        startup: Option<String>,
        use_sequence: u64,
    ) -> Self {
        Self {
            held: records
                .iter()
                .filter(|record| matches!(record.location, Location::Loaded))
                .count()
                + usize::from(startup.is_some()),
            records,
            startup,
            use_sequence,
            cancels: BTreeMap::new(),
            settled_replies: BTreeMap::new(),
        }
    }
    #[cfg(test)]
    pub(super) fn startup(&self) -> Option<&str> {
        self.startup.as_deref()
    }
}
fn select<'a>(
    records: &[Record],
    hashes: &RecordHashes,
    tools: ToolSetHash,
    items: &'a [&'a Item],
) -> Option<(usize, &'a [&'a Item])> {
    records
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            record.record.tools == tools
                && record.record.hashes.is_continued_by(hashes)
                && record
                    .record
                    .last_reply
                    .matches(&items[record.record.hashes.messages.len()..])
                && conversation::continues_after(items, record.record.len())
        })
        .max_by_key(|(_, record)| (record.record.len(), record.record.used))
        .map(|(index, record)| (index, &items[record.record.len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{Conversation, Part, Role};

    fn session_use(selection: &Selection) -> &'static str {
        match selection.kind {
            crate::journal::SessionUse::Continue => "continue",
            crate::journal::SessionUse::Rebuild => "rebuild",
            crate::journal::SessionUse::New => "new",
        }
    }

    #[test]
    fn issue218_last_tool_call_compares_id_and_name_without_arguments() {
        for (id, name, arguments, expected) in [
            ("call-id", "Bash", r#"{"command":"make"}"#, "continue"),
            (
                "call-id",
                "Bash",
                r#"{"command":"cd /work && make"}"#,
                "continue",
            ),
            ("other", "Bash", r#"{"command":"make"}"#, "rebuild"),
            ("call-id", "Read", r#"{"command":"make"}"#, "rebuild"),
        ] {
            let first = turn_a();
            let settings = SessionSettings::for_turn(&first);
            let mut sessions = AcpSessions::default();
            let (selection, _) = sessions.select(&first, &settings, None, 0);
            sessions.keep(
                "recorded".into(),
                Some(
                    selection
                        .record
                        .with_reply("", Some(("call-id".into(), "Bash".into()))),
                ),
            );
            let next = chat_turn(serde_json::json!({"model":"m", "messages":[
                {"role":"user","content":"A"},
                {"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":arguments}}]},
                {"role":"tool","tool_call_id":id,"content":"Done."}
            ]}));
            let (selection, _) = sessions.select(&next, &settings, None, 1);
            assert_eq!(
                session_use(&selection),
                expected,
                "last call {id} {name} {arguments}"
            );
            if expected == "continue" {
                assert!(
                    matches!(&selection.prompt[..], [conversation::PromptBlock::Text(text)]
                    if text == &format!("Result of your tool call {name} {arguments}:\nDone.")),
                    "FR-023 sends one result with the request's arguments"
                );
            }
        }
    }

    #[test]
    fn issue218_only_last_reply_text_uses_clean_form() {
        use serde_json::json;
        let assistant = |text: &str| json!({"role":"assistant","content":text});
        for (reply, returned, expected) in [
            (
                "\n  if a:\r\n    return  \t\n\n",
                assistant("if a:\n    return"),
                "continue",
            ),
            ("one\rcarriage", assistant("onecarriage"), "continue"),
            ("one\u{2003}\ntwo\u{a0}", assistant("one\ntwo"), "continue"),
            ("if a:\n    return", assistant("if a:\n  return"), "rebuild"),
            ("name =  value", assistant("name = value"), "rebuild"),
            (" \n\t", assistant(""), "continue"),
            ("R", json!({"role":"user","content":"R"}), "rebuild"),
            (
                "R",
                json!({"role":"assistant","name":"bob","content":"R"}),
                "rebuild",
            ),
            (
                "R",
                json!({"role":"assistant","content":[{"type":"text","text":"R"},{"type":"text","text":""}]}),
                "rebuild",
            ),
        ] {
            let first = turn_a();
            let settings = SessionSettings::for_turn(&first);
            let mut sessions = AcpSessions::default();
            let (selection, _) = sessions.select(&first, &settings, None, 0);
            sessions.keep(
                "recorded".into(),
                Some(selection.record.with_reply(reply, None)),
            );
            let next = chat_turn(serde_json::json!({"model":"m", "messages":[
                {"role":"user","content":"A"},
                returned,
                {"role":"user","content":"Next"}
            ]}));
            let (selection, _) = sessions.select(&next, &settings, None, 1);
            assert_eq!(
                session_use(&selection),
                expected,
                "last reply {reply:?} -> {returned:?}"
            );
        }
    }

    #[test]
    fn issue218_a_match_keeps_the_client_form_for_later_exact_comparisons() {
        for (later, expected) in [("one\ntwo", "continue"), ("one \ntwo", "rebuild")] {
            let first = turn_a();
            let settings = SessionSettings::for_turn(&first);
            let mut sessions = AcpSessions::default();
            let (selection, _) = sessions.select(&first, &settings, None, 0);
            sessions.keep(
                "recorded".into(),
                Some(selection.record.with_reply("one  \ntwo", None)),
            );
            let next = chat_turn(serde_json::json!({"model":"m", "messages":[
                {"role":"user","content":"A"},
                {"role":"assistant","content":"one\ntwo"},
                {"role":"user","content":"Next"}
            ]}));
            let (selection, _) = sessions.select(&next, &settings, None, 1);
            assert_eq!(
                session_use(&selection),
                "continue",
                "first client form matches"
            );
            sessions.keep(
                "recorded".into(),
                Some(selection.record.with_reply("Fine.", None)),
            );
            let later = chat_turn(serde_json::json!({"model":"m", "messages":[
                {"role":"user","content":"A"},
                {"role":"assistant","content":later},
                {"role":"user","content":"Next"},
                {"role":"assistant","content":"Fine."},
                {"role":"user","content":"Again"}
            ]}));
            let (selection, _) = sessions.select(&later, &settings, None, 2);
            assert_eq!(
                session_use(&selection),
                expected,
                "accepted client form is exact later"
            );
        }
    }

    #[test]
    fn issue218_earlier_items_keep_exact_white_space() {
        for (sent, returned, expected) in [
            ("if a:\n    return", "if a:\n  return", "rebuild"),
            ("one  \ntwo", "one\ntwo", "rebuild"),
            ("same  text", "same  text", "continue"),
        ] {
            let first = chat_turn(serde_json::json!({"model":"m", "messages":[
                {"role":"user","content":sent}
            ]}));
            let settings = SessionSettings::for_turn(&first);
            let mut sessions = AcpSessions::default();
            let (selection, _) = sessions.select(&first, &settings, None, 0);
            sessions.keep(
                "recorded".into(),
                Some(selection.record.with_reply("R", None)),
            );
            let next = chat_turn(serde_json::json!({"model":"m", "messages":[
                {"role":"user","content":returned},
                {"role":"assistant","content":"R"},
                {"role":"user","content":"Next"}
            ]}));
            let (selection, _) = sessions.select(&next, &settings, None, 1);
            let actual = match selection.kind {
                crate::journal::SessionUse::Continue => "continue",
                crate::journal::SessionUse::Rebuild => "rebuild",
                crate::journal::SessionUse::New => "new",
            };
            assert_eq!(
                actual, expected,
                "earlier user text {sent:?} -> {returned:?}"
            );
        }
    }

    #[test]
    fn expired_reply_summaries_are_removed_at_the_next_input() {
        let mut sessions = AcpSessions::default();
        for i in 0..1000 {
            sessions.remember_reply(format!("s{i}"), Default::default(), 100);
        }
        sessions.expire(100);
        assert_eq!(sessions.settled_replies.len(), 1000);
        sessions.expire(101);
        assert!(
            sessions.settled_replies.is_empty(),
            "expired summaries must not accumulate"
        );
    }

    fn chat_turn(body: serde_json::Value) -> crate::conversation::TurnRequest {
        crate::dialect::CHAT
            .parse(&serde_json::to_vec(&body).unwrap())
            .ok()
            .unwrap()
            .turn
    }

    fn turn_a() -> crate::conversation::TurnRequest {
        chat_turn(serde_json::json!({
            "model": "m",
            "messages": [{"role": "user", "content": "A"}]
        }))
    }

    fn turn_a_r_b() -> crate::conversation::TurnRequest {
        chat_turn(serde_json::json!({
            "model": "m",
            "messages": [
                {"role": "user", "content": "A"},
                {"role": "assistant", "content": "R"},
                {"role": "user", "content": "B"}
            ]
        }))
    }

    #[test]
    fn longest_record_precedes_recency() {
        fn user(text: &str) -> Item {
            Item::Message {
                role: Role::User,
                name: None,
                parts: vec![Part::Text(text.into())],
            }
        }
        let conversation = Conversation {
            system: vec![],
            earlier: vec![user("A"), user("B")],
            last: user("C"),
        };
        let hashes = conversation::record_hashes(&conversation);
        let tools = conversation::tool_set_hash(&[]);
        let records = vec![
            Record {
                location: Location::Loaded,
                session: "short-recent".into(),
                record: RequestRecord {
                    last_reply: LastReply::default(),
                    hashes: RecordHashes {
                        system: hashes.system,
                        messages: hashes.messages[..1].into(),
                    },
                    tools,
                    used: 99,
                    expires: 0,
                },
            },
            Record {
                location: Location::Loaded,
                session: "long-older".into(),
                record: RequestRecord {
                    last_reply: LastReply::default(),
                    hashes: RecordHashes {
                        system: hashes.system,
                        messages: hashes.messages[..2].into(),
                    },
                    tools,
                    used: 1,
                    expires: 0,
                },
            },
        ];
        let items = conversation.items();
        assert_eq!(
            select(&records, &hashes, tools, &items).map(|(index, _)| index),
            Some(1),
            "ADR-0003 longest eligible record wins before recency"
        );
    }

    #[test]
    fn a_later_cancel_keeps_the_first_cancellation_deadline() {
        let mut sessions = AcpSessions::default();
        sessions.cancel("running".into(), 1_000);
        sessions.cancel("running".into(), 20_000);
        assert!(!sessions.cancellation_due(30_999));
        assert!(
            sessions.cancellation_due(31_000),
            "FR-041 the first cancel sets the 30 s deadline"
        );
    }

    #[test]
    fn the_session_used_last_continues_among_records_of_the_same_length() {
        let first_turn = turn_a();
        let settings = SessionSettings::for_turn(&first_turn);
        let mut sessions = AcpSessions::default();
        let (used_first, _) = sessions.select(&first_turn, &settings, None, 0);
        let (used_last, _) = sessions.select(&first_turn, &settings, None, 0);
        sessions.keep(
            "used-last".into(),
            Some(used_last.record.with_reply("R", None)),
        );
        sessions.keep(
            "used-first".into(),
            Some(used_first.record.with_reply("R", None)),
        );
        let next_turn = turn_a_r_b();
        let (selection, _) = sessions.select(&next_turn, &settings, None, 0);
        assert_eq!(
            selection.ready.map(|(session, _)| session).as_deref(),
            Some("used-last"),
            "ADR-0003 the ACP session used last wins among records of the same length"
        );
    }

    #[test]
    fn a_forgotten_startup_session_is_not_used_again() {
        for (forgotten, expected) in [("other", Some("startup")), ("startup", None)] {
            let turn = turn_a();
            let settings = SessionSettings::for_turn(&turn);
            let mut sessions = AcpSessions::fixture(Vec::new(), Some("startup".into()), 0);
            sessions.forget(forgotten.into());
            let (selection, _) = sessions.select(&turn, &settings, None, 0);
            assert_eq!(
                selection.ready.map(|(session, _)| session).as_deref(),
                expected,
                "a forget of {forgotten}"
            );
        }
    }

    #[test]
    fn a_runtime_failure_drops_the_startup_session() {
        for (failed, expected) in [(false, Some("startup")), (true, None)] {
            let turn = turn_a();
            let settings = SessionSettings::for_turn(&turn);
            let mut sessions = AcpSessions::fixture(Vec::new(), Some("startup".into()), 0);
            if failed {
                sessions.runtime_failed();
            }
            let (selection, _) = sessions.select(&turn, &settings, None, 0);
            assert_eq!(
                selection.ready.map(|(session, _)| session).as_deref(),
                expected,
                "failed={failed}"
            );
        }
    }

    #[test]
    fn a_checked_session_is_forgotten_when_a_record_continues_the_request() {
        let first_turn = turn_a();
        let settings = SessionSettings::for_turn(&first_turn);
        let mut sessions = AcpSessions::default();
        let (selection, _) = sessions.select(&first_turn, &settings, None, 0);
        sessions.keep(
            "recorded".into(),
            Some(selection.record.with_reply("R", None)),
        );
        let next_turn = turn_a_r_b();
        let (selection, commands) =
            sessions.select(&next_turn, &settings, Some("checked".into()), 0);
        assert_eq!(
            selection.ready.map(|(session, _)| session).as_deref(),
            Some("recorded")
        );
        assert!(
            matches!(&commands[..], [Command::Forget(session)] if session == "checked"),
            "the unused checked session is forgotten"
        );
    }
}
