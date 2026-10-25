use serde::Serialize;
use std::{io, path::Path, time::SystemTime};

#[derive(Clone, Copy, Debug, Serialize)]
pub enum Api {
    #[serde(rename = "models")]
    Models,
    #[serde(rename = "chat.completions")]
    Chat,
    #[serde(rename = "messages")]
    Messages,
    #[serde(rename = "responses")]
    Responses,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionUse {
    New,
    Continue,
    Rebuild,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultKind {
    Success,
    Refused,
    Failed,
    Disconnected,
}
#[derive(Clone, Debug, Serialize)]
pub struct RequestRecord {
    pub time: String,
    pub request_id: String,
    pub api: Api,
    pub model: Option<String>,
    pub session: Option<String>,
    pub session_use: Option<SessionUse>,
    pub duration_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub result: ResultKind,
    pub error_code: Option<String>,
    pub unknown_fields: Vec<String>,
}
const FILE_CAP: u64 = 10_000_000;
const TOTAL_CAP: u64 = 50_000_000;
const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 60 * 60);
struct LogFile {
    path: std::path::PathBuf,
    modified: SystemTime,
    size: u64,
    date: String,
    sequence: u64,
}
pub struct Writer {
    folder: std::path::PathBuf,
    date: String,
    sequence: u64,
    file: std::fs::File,
    size: u64,
}
impl Writer {
    pub fn open(folder: &Path, wall: SystemTime) -> io::Result<Self> {
        std::fs::create_dir_all(folder)?;
        if std::fs::symlink_metadata(folder)?.file_type().is_symlink() {
            return Err(io::Error::other("log folder is a symbolic link"));
        }
        let date = timestamp(wall)?[..10].to_owned();
        let mut sequence = 0;
        for file in log_files(folder)? {
            if file.date == date {
                sequence = sequence.max(file.sequence);
            }
            if wall
                .duration_since(file.modified)
                .is_ok_and(|age| age > MAX_AGE)
                && let Err(error) = std::fs::remove_file(&file.path)
            {
                eprintln!(
                    "acp2api: cannot delete the old log file {}: {error}",
                    file.path.display()
                );
            }
        }
        let (file, sequence) = create_file(folder, &date, sequence)?;
        let mut writer = Self {
            folder: folder.to_owned(),
            date,
            sequence,
            file,
            size: 0,
        };
        writer.retain()?;
        Ok(writer)
    }
    pub fn write(&mut self, record: &impl Serialize) -> io::Result<usize> {
        let mut line = serde_json::to_vec(record)?;
        line.push(b'\n');
        if line.len() as u64 > FILE_CAP {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "request metadata exceeds the log file cap",
            ));
        }
        if self.size + line.len() as u64 > FILE_CAP {
            let (file, sequence) = create_file(&self.folder, &self.date, self.sequence)?;
            self.file = file;
            self.sequence = sequence;
            self.size = 0;
        }
        use std::io::Write;
        self.file.write_all(&line)?;
        self.file.flush()?;
        self.size += line.len() as u64;
        self.retain()?;
        Ok(line.len())
    }
    fn retain(&mut self) -> io::Result<()> {
        let mut files = log_files(&self.folder)?;
        files.sort_by(|a, b| {
            (a.modified, &a.date, a.sequence).cmp(&(b.modified, &b.date, b.sequence))
        });
        let mut size: u64 = files.iter().map(|f| f.size).sum();
        for file in files {
            if size <= TOTAL_CAP {
                break;
            }
            if file.date == self.date && file.sequence == self.sequence {
                continue;
            }
            std::fs::remove_file(file.path)?;
            size -= file.size;
        }
        Ok(())
    }
}
fn create_file(folder: &Path, date: &str, mut sequence: u64) -> io::Result<(std::fs::File, u64)> {
    loop {
        sequence += 1;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(folder.join(format!("acp2api-{date}-{sequence}.log")))
        {
            Ok(file) => return Ok((file, sequence)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}
fn log_files(folder: &Path) -> io::Result<Vec<LogFile>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name
            .to_str()
            .and_then(|s| s.strip_prefix("acp2api-"))
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        let Some((date, sequence)) = name.rsplit_once('-') else {
            continue;
        };
        if date.len() != 10
            || !date.bytes().enumerate().all(|(i, c)| {
                if i == 4 || i == 7 {
                    c == b'-'
                } else {
                    c.is_ascii_digit()
                }
            })
        {
            continue;
        }
        let Some(sequence) = sequence.parse::<u64>().ok().filter(|n| *n > 0) else {
            continue;
        };
        let meta = std::fs::symlink_metadata(entry.path())?;
        files.push(LogFile {
            path: entry.path(),
            modified: meta.modified()?,
            size: meta.len(),
            date: date.into(),
            sequence,
        });
    }
    Ok(files)
}
pub(crate) fn timestamp(wall: SystemTime) -> io::Result<String> {
    let seconds = wall
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs();
    let mut days = seconds / 86_400;
    let mut year = 1970;
    let leap = |year: u64| {
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
    };
    loop {
        let count = if leap(year) { 366 } else { 365 };
        if days < count {
            break;
        }
        days -= count;
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= months[month] {
        days -= months[month];
        month += 1;
    }
    Ok(format!(
        "{year:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        month + 1,
        days + 1,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    ))
}

#[derive(Clone)]
pub(crate) struct Journal(std::sync::Arc<JournalState>);
struct JournalState {
    writer: std::sync::Mutex<Writer>,
    prefix: String,
    sequence: std::sync::atomic::AtomicU64,
}
#[derive(Clone)]
pub(crate) struct Request {
    pub id: String,
    pub api: Api,
    pub model: Option<String>,
    pub unknown_fields: Vec<String>,
    started: crate::Moment,
}
pub(crate) enum Outcome {
    ModelList,
    Success { input: u64, output: u64 },
    Refused(&'static str),
    Failed,
    BuiltInToolRan,
    Disconnected,
}
pub(crate) struct Completion {
    pub request: Request,
    pub session: Option<String>,
    pub session_use: Option<SessionUse>,
    pub outcome: Outcome,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SafetyKind {
    PermissionRefused,
    BuiltInToolRan,
}
pub(crate) struct SafetyReport {
    pub request: Request,
    pub session: String,
    pub kind: SafetyKind,
    pub tool: String,
}
#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(crate) enum Diagnostic {
    UnknownEffort { value: serde_json::Value },
    DroppedThoughtText,
}
fn redact_error(text: &str) -> String {
    let token_char = |c: char| c.is_ascii_alphanumeric() || "-_.+/=".contains(c);
    let word_char = |c: char| c.is_alphanumeric() || "-_.+/=@%!#$*^`{|}~".contains(c);
    let mut output = String::new();
    let mut offset = 0;
    for part in text.split_inclusive(|c| !word_char(c)) {
        let word = part.trim_end_matches(|c| !word_char(c));
        let email = word
            .split_once('@')
            .or_else(|| word.split_once("%40"))
            .filter(|(local, domain)| {
                !local.is_empty()
                    && !domain.is_empty()
                    && domain
                        .chars()
                        .all(|c| c.is_alphanumeric() || "-.".contains(c))
            })
            .map(|(local, _)| local.rfind('=').map_or(0, |at| at + 1));
        if !word.is_empty() && text[..offset].ends_with("Bearer ") {
            output.push_str("<REDACTED>");
        } else {
            for segment in word[..email.unwrap_or(word.len())].split_inclusive(|c| !token_char(c)) {
                let value = segment.trim_end_matches(|c| !token_char(c));
                output.push_str(if value.len() >= 32 {
                    "<REDACTED>"
                } else {
                    value
                });
                output.push_str(&segment[value.len()..]);
            }
            if email.is_some() {
                output.push_str("<REDACTED>");
            }
        }
        output.push_str(&part[word.len()..]);
        offset += part.len();
    }
    output
}
impl Journal {
    pub(crate) fn upstream(
        &self,
        session: &str,
        error: &crate::messages::HttpError,
        late: bool,
        clock: &dyn crate::Clock,
    ) {
        let result = timestamp(clock.wall()).and_then(|time| self.emit_to_file_then_stdout_under_writer_lock(&serde_json::json!({
            "time": time,
            "session": session.chars().take(12).collect::<String>(),
            "event": if late { "late_upstream_error" } else { "upstream_error" },
            "code": error.code, "status": redact_error(&error.status), "message": redact_error(&error.message),
        })));
        if let Err(error) = result {
            eprintln!("acp2api: cannot write upstream error: {error}");
        }
    }
    pub(crate) fn diagnostic(
        &self,
        request: &Request,
        diagnostic: Diagnostic,
        clock: &dyn crate::Clock,
    ) {
        if let Err(error) = self.diagnostic_line(request, diagnostic, clock) {
            eprintln!("acp2api: cannot write request diagnostic: {error}");
        }
    }
    fn diagnostic_line(
        &self,
        request: &Request,
        diagnostic: Diagnostic,
        clock: &dyn crate::Clock,
    ) -> io::Result<()> {
        let mut record = serde_json::to_value(diagnostic)?;
        record["time"] = timestamp(clock.wall())?.into();
        record["request_id"] = request.id.clone().into();
        self.emit_to_file_then_stdout_under_writer_lock(&record)
    }
    pub(crate) fn safety(&self, report: SafetyReport, clock: &dyn crate::Clock) {
        if let Err(error) = self.safety_line(report, clock) {
            eprintln!("acp2api: cannot write runtime safety event: {error}");
        }
    }
    fn safety_line(&self, report: SafetyReport, clock: &dyn crate::Clock) -> io::Result<()> {
        let record = serde_json::json!({
            "time":timestamp(clock.wall())?,
            "request_id":report.request.id,
            "session":report.session.chars().take(12).collect::<String>(),
            "event":report.kind,
            "tool":report.tool,
        });
        self.emit_to_file_then_stdout_under_writer_lock(&record)
    }
    fn emit_to_file_then_stdout_under_writer_lock(
        &self,
        record: &impl Serialize,
    ) -> io::Result<()> {
        let mut writer = self
            .0
            .writer
            .lock()
            .map_err(|_| io::Error::other("journal writer failed"))?;
        writer.write(record)?;
        use std::io::Write;
        let mut terminal = std::io::stdout().lock();
        writeln!(terminal, "acp2api: {}", serde_json::to_string(record)?)?;
        terminal.flush()
    }
    pub(crate) fn open(folder: &Path, wall: SystemTime) -> io::Result<Self> {
        let prefix = crate::random_hex::<8>().map_err(io::Error::other)?;
        Ok(Self(std::sync::Arc::new(JournalState {
            writer: std::sync::Mutex::new(Writer::open(folder, wall)?),
            prefix,
            sequence: std::sync::atomic::AtomicU64::new(1),
        })))
    }
    pub(crate) fn begin(&self, api: Api, started: crate::Moment) -> Request {
        let n = self
            .0
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Request {
            id: format!("req_{}_{n}", self.0.prefix),
            api,
            model: None,
            unknown_fields: Vec::new(),
            started,
        }
    }
    pub(crate) fn complete(&self, completion: Completion, clock: &dyn crate::Clock) {
        if let Err(error) = self.write_line(completion, clock) {
            eprintln!("acp2api: cannot write request metadata: {error}");
        }
    }
    fn write_line(&self, completion: Completion, clock: &dyn crate::Clock) -> io::Result<()> {
        let Completion {
            request,
            session,
            session_use,
            outcome,
        } = completion;
        let (result, error_code, input_tokens, output_tokens) = match outcome {
            Outcome::ModelList => (ResultKind::Success, None, None, None),
            Outcome::Success { input, output } => {
                (ResultKind::Success, None, Some(input), Some(output))
            }
            Outcome::Refused(code) => (ResultKind::Refused, Some(code), None, None),
            Outcome::Failed => (ResultKind::Failed, Some("upstream_error"), None, None),
            Outcome::BuiltInToolRan => (ResultKind::Failed, Some("built_in_tool_ran"), None, None),
            Outcome::Disconnected => (
                ResultKind::Disconnected,
                Some("client_disconnected"),
                None,
                None,
            ),
        };
        let session = session.map(|id| id.chars().take(12).collect());
        let record = RequestRecord {
            time: timestamp(clock.wall())?,
            request_id: request.id,
            api: request.api,
            model: request.model,
            session,
            session_use,
            duration_ms: clock.now().saturating_sub(request.started),
            input_tokens,
            output_tokens,
            result,
            error_code: error_code.map(str::to_owned),
            unknown_fields: request.unknown_fields,
        };
        self.emit_to_file_then_stdout_under_writer_lock(&record)
    }
}
#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::Clock;
    use std::time::Duration;
    fn folder(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("acp2api-journal-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }
    fn log(folder: &Path, name: &str, size: u64, modified: SystemTime) -> std::path::PathBuf {
        let path = folder.join(name);
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(size).unwrap();
        file.set_modified(modified).unwrap();
        path
    }
    fn record(id: &str) -> RequestRecord {
        RequestRecord {
            time: "2026-10-22T00:00:00Z".into(),
            request_id: id.into(),
            api: Api::Chat,
            model: None,
            session: None,
            session_use: None,
            duration_ms: 0,
            input_tokens: None,
            output_tokens: None,
            result: ResultKind::Success,
            error_code: None,
            unknown_fields: Vec::new(),
        }
    }
    #[test]
    fn quoted_email_is_redacted() {
        assert_eq!(redact_error("'a@example.com'"), "'<REDACTED>'");
    }
    #[test]
    fn email_in_a_query_value_is_redacted() {
        assert_eq!(
            redact_error("?Email=name%40example.com&y=1"),
            "?Email=<REDACTED>&y=1"
        );
    }
    #[test]
    fn upstream_logs_redact_values_inside_punctuation_without_changing_the_error() {
        let folder = folder("upstream-redaction");
        let clock = crate::os::SystemClock::default();
        let journal = Journal::open(&folder, clock.wall()).unwrap();
        let error = crate::messages::HttpError {
            code: 503, status: "UNAVAILABLE".into(),
            message: "Account (first.last+tag@sub.example.org), second%40example.org; Header Bearer short; Key ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ.".into(),
        };
        journal.upstream("s1", &error, false, &clock);
        let file = std::fs::read_dir(&folder)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(
            record["message"],
            "Account (<REDACTED>), <REDACTED>; Header Bearer <REDACTED>; Key <REDACTED>"
        );
        assert!(error.message.contains("first.last+tag@sub.example.org"));
        drop(journal);
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn retention_keeps_the_active_file_after_the_clock_went_back() {
        let folder = folder("clock");
        let now = SystemTime::now();
        let later = now + Duration::from_secs(86_400);
        for day in 1..=5 {
            log(
                &folder,
                &format!("acp2api-2026-01-0{day}-1.log"),
                9_000_000,
                later,
            );
        }
        let mut writer = Writer::open(&folder, now).unwrap();
        writer.write(&record("first-line")).unwrap();
        log(&folder, "acp2api-2026-01-06-1.log", 9_000_000, later);
        writer.write(&record("second-line")).unwrap();
        let active = folder.join(format!("acp2api-{}-1.log", &timestamp(now).unwrap()[..10]));
        let text = std::fs::read_to_string(active).unwrap_or_default();
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(text.contains("first-line") && text.contains("second-line"));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn a_stale_log_that_cannot_be_deleted_does_not_stop_the_start() {
        use std::os::unix::ffi::OsStrExt;
        let folder = folder("stale");
        let old = SystemTime::now() - Duration::from_secs(31 * 86_400);
        let path = log(&folder, "acp2api-2026-01-01-1.log", 10, old);
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(
            unsafe { libc::chflags(c_path.as_ptr(), libc::UF_IMMUTABLE) },
            0
        );
        let opened = Writer::open(&folder, SystemTime::now());
        assert_eq!(unsafe { libc::chflags(c_path.as_ptr(), 0) }, 0);
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(opened.is_ok(), "{:?}", opened.err());
    }
}
