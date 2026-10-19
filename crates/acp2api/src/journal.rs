use serde::Serialize;
use std::{io, path::Path, time::SystemTime};

#[derive(Clone, Copy, Debug, Serialize)]
pub enum Api {
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
            {
                std::fs::remove_file(file.path)?;
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
    pub fn write(&mut self, record: &RequestRecord) -> io::Result<usize> {
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
            std::fs::remove_file(file.path)?;
            size -= file.size;
        }
        Ok(())
    }
}
fn create_file(folder: &Path, date: &str, mut sequence: u64) -> io::Result<(std::fs::File, u64)> {
    loop {
        sequence = sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("log file sequence exhausted"))?;
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
        let meta = entry.metadata()?;
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
    if seconds > 253_402_300_799 {
        return Err(io::Error::other("wall time is outside the RFC 3339 range"));
    }
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
