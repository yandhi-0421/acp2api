#![allow(
    clippy::disallowed_methods,
    reason = "Real-time safety caps must work while virtual time is blocked"
)]
use acp2api_acceptance::real_limit;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

// At most 3 requests, a 5 s run, 3 s socket I/O, 16 KiB/request, no log.
pub struct Endpoint {
    pub url: String,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Endpoint {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let got = count.clone();
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + real_limit(Duration::from_secs(5));
            while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => panic!("MCP fixture accept: {error}"),
                };
                assert!(
                    got.fetch_add(1, Ordering::SeqCst) < 3,
                    "MCP fixture request cap: 3"
                );
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(real_limit(Duration::from_secs(3))))
                    .unwrap();
                stream
                    .set_write_timeout(Some(real_limit(Duration::from_secs(3))))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut length = 0;
                let mut size = 0;
                loop {
                    let mut line = String::new();
                    let read = reader
                        .by_ref()
                        .take(16 * 1024 + 1)
                        .read_line(&mut line)
                        .unwrap();
                    size += read;
                    assert!(size <= 16 * 1024 && read > 0, "MCP request head cap");
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                assert!(size + length <= 16 * 1024, "MCP request cap: 16 KiB");
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let response = r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
            }
        });
        Self {
            url,
            count,
            stop,
            worker: Some(worker),
        }
    }
    pub fn requests(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
