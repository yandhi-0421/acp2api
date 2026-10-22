use acp2api::journal::{Api, RequestRecord, ResultKind, Writer};
use acp2api_acceptance::setup::TestRoot;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn files(folder: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}

fn record(id: &str, padding: usize) -> RequestRecord {
    RequestRecord {
        time: "2026-10-18T00:00:00Z".into(),
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
        unknown_fields: vec!["x".repeat(padding)],
    }
}

#[test]
#[allow(clippy::disallowed_methods)]
fn successful_write_after_retention_with_future_files_remains_readable() {
    let root = TestRoot::new();
    let folder = root.path.join("clock-correction-logs");
    let deadline = Instant::now() + Duration::from_secs(20);
    let wall = UNIX_EPOCH + Duration::from_secs(1_792_281_600);
    let future = SystemTime::now() + Duration::from_secs(365 * 24 * 60 * 60);
    let mut input = 0;
    let mut count = 0;
    for n in 0..5 {
        assert!(
            Instant::now() < deadline,
            "writer investigation: 20 seconds"
        );
        let mut writer = Writer::open(&folder, wall).unwrap();
        input += writer
            .write(&record(&format!("closed-{n}"), 9_000_000))
            .unwrap();
        count += 1;
        drop(writer);
        let closed = files(&folder).pop().unwrap();
        fs::File::options()
            .write(true)
            .open(&closed)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(future))
            .unwrap();
        assert_eq!(fs::metadata(closed).unwrap().modified().unwrap(), future);
        root.check_size();
    }
    let closed = files(&folder);
    assert_eq!(closed.len(), 5, "five real closed writer files");
    let mut writer = Writer::open(&folder, wall).unwrap();
    let active = files(&folder)
        .into_iter()
        .find(|path| !closed.contains(path))
        .unwrap();
    #[cfg(unix)]
    let mut observer = fs::File::open(&active).unwrap();
    input += writer
        .write(&record("capacity-trigger", 9_000_000))
        .unwrap();
    count += 1;
    let active_exists = active.exists();
    let retained_bytes: u64 = files(&folder)
        .iter()
        .map(|path| fs::metadata(path).unwrap().len())
        .sum();
    let marker = record("subsequent-successful-write", 0);
    let marker_size = serde_json::to_vec(&marker).unwrap().len() + 1;
    assert!(retained_bytes + marker_size as u64 <= 50_000_000);
    let successful_bytes = writer.write(&marker).unwrap();
    assert_eq!(successful_bytes, marker_size);
    input += successful_bytes;
    count += 1;
    #[cfg(unix)]
    {
        use std::io::{Read, Seek, SeekFrom};
        use std::os::unix::fs::MetadataExt;
        let links = observer.metadata().unwrap().nlink();
        observer.seek(SeekFrom::End(-(marker_size as i64))).unwrap();
        let mut tail = String::new();
        observer.read_to_string(&mut tail).unwrap();
        let marker_in_observed_inode = tail.contains("subsequent-successful-write");
        println!(
            "observed_inode_links={links} marker_in_observed_inode={marker_in_observed_inode}"
        );
        drop(observer);
    }
    drop(writer);
    let marker_present = files(&folder).iter().any(|path| {
        fs::read_to_string(path).unwrap().lines().any(|line| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            value["request_id"] == "subsequent-successful-write"
        })
    });
    assert!(input <= 80 * 1024 * 1024 && count <= 80);
    assert!(
        Instant::now() < deadline,
        "writer investigation: 20 seconds"
    );
    root.check_size();
    let summary = format!(
        "records={count} input_bytes={input} active_exists_after_retention={active_exists} retained_bytes_before_marker={retained_bytes} successful_marker_bytes={successful_bytes} marker_present={marker_present}"
    );
    assert!(summary.len() <= 65_536);
    println!("{summary}");
    root.cleanup();
    assert!(
        marker_present,
        "a successful subsequent write without capacity pressure must remain in a retained log file"
    );
}
