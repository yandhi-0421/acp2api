use super::*;
use futures_util::task::AtomicWaker;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::AsyncWrite;
use tokio::sync::watch;

pub(crate) struct Recording {
    pub(crate) bytes: Mutex<Vec<u8>>,
    limit: AtomicUsize,
    waker: AtomicWaker,
    closed: AtomicBool,
}
impl Recording {
    pub(crate) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            bytes: Mutex::new(Vec::new()),
            limit: AtomicUsize::new(limit),
            waker: AtomicWaker::new(),
            closed: AtomicBool::new(false),
        })
    }
    pub(crate) fn allow(&self, limit: usize) {
        self.limit.store(limit, Ordering::SeqCst);
        self.waker.wake();
    }
}
pub(crate) struct Pipe(pub(crate) Arc<Recording>);
impl AsyncWrite for Pipe {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.waker.register(cx.waker());
        let mut recorded = self.0.bytes.lock().unwrap();
        let count = self
            .0
            .limit
            .load(Ordering::SeqCst)
            .saturating_sub(recorded.len())
            .min(bytes.len());
        if count == 0 {
            return Poll::Pending;
        }
        assert!(
            recorded.len() + count <= 16 * 1024,
            "recording pipe capture cap: 16 KiB"
        );
        recorded.extend_from_slice(&bytes[..count]);
        Poll::Ready(Ok(count))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}
impl Drop for Pipe {
    fn drop(&mut self) {
        self.0.closed.store(true, Ordering::SeqCst);
    }
}
pub(crate) struct TestClock {
    now: watch::Sender<Moment>,
    waits: mpsc::UnboundedSender<Moment>,
    holds: Arc<AtomicUsize>,
    registrations: Arc<Mutex<Vec<(Moment, bool)>>>,
}
impl TestClock {
    pub(crate) fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<Moment>) {
        let (waits, deadlines) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                now: watch::channel(0).0,
                waits,
                holds: Arc::new(AtomicUsize::new(0)),
                registrations: Arc::default(),
            }),
            deadlines,
        )
    }
    pub(crate) async fn advance_to(&self, at: Moment) {
        loop {
            tokio::task::yield_now().await;
            if self.holds.load(Ordering::SeqCst) == 0 {
                self.now.send_replace(at);
                return;
            }
        }
    }
}
impl Clock for TestClock {
    fn now(&self) -> Moment {
        *self.now.borrow()
    }
    fn wall(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_millis(self.now())
    }
    fn hold(&self, _: HoldKind) -> Hold {
        self.holds.fetch_add(1, Ordering::SeqCst);
        let holds = self.holds.clone();
        Hold::on_drop(move || {
            holds.fetch_sub(1, Ordering::SeqCst);
        })
    }
    fn sleep_until(&self, at: Moment) -> BoxFuture<'static, Hold> {
        let mut registrations = self.registrations.lock().unwrap();
        assert!(registrations.len() < 32, "clock capture cap: 32 deadlines");
        registrations.push((at, self.holds.load(Ordering::SeqCst) > 0));
        self.waits.send(at).unwrap();
        let mut now = self.now.subscribe();
        Box::pin(async move {
            now.wait_for(|now| *now >= at).await.unwrap();
            Hold::empty()
        })
    }
}
struct Tree;
impl ProcessTree for Tree {
    fn pid(&self) -> u32 {
        std::process::id()
    }
    fn exited(&mut self) -> BoxFuture<'_, ExitStatus> {
        Box::pin(std::future::pending())
    }
    fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}
struct GraceTree;
impl ProcessTree for GraceTree {
    fn pid(&self) -> u32 {
        std::process::id()
    }
    fn exited(&mut self) -> BoxFuture<'_, ExitStatus> {
        Box::pin(std::future::pending())
    }
    fn stop(self: Box<Self>, clock: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let _work = clock.sleep_until(clock.now() + 5_000).await;
        })
    }
}
pub(crate) struct Launcher(pub(crate) Mutex<Option<Launched>>);
impl ProcessLauncher for Launcher {
    fn launch(&self, _: LaunchSpec) -> Result<Launched, LaunchError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .take()
            .expect("one recording pipe cap"))
    }
}
struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(crate) struct Fixture {
    pub(crate) runtime: Runtime,
    pub(crate) start: RuntimeStart,
    pub(crate) now: watch::Sender<Moment>,
    pub(crate) waits: mpsc::UnboundedReceiver<Moment>,
    recording: Arc<Recording>,
    clock: Arc<TestClock>,
    _stdout: tokio::io::DuplexStream,
    _root: Root,
}
pub(crate) async fn fixture() -> Fixture {
    fixture_with_stderr(Box::pin(tokio::io::empty())).await
}
async fn fixture_with_stderr(stderr: ProcessRead) -> Fixture {
    let root = Root(Environment::from_process().temp.join(format!(
        "acp2api-write-{}",
        crate::random_hex::<8>().unwrap()
    )));
    let (clock, waiting) = TestClock::new();
    let now = clock.now.clone();
    let recording = Recording::new(16);
    let (stdout, output) = tokio::io::duplex(1024);
    let ports = Ports::production();
    let start = RuntimeStart {
        stopped: watch::channel(false).1,
        clock: clock.clone(),
        launcher: Arc::new(Launcher(Mutex::new(Some(Launched {
            stdin: Box::pin(Pipe(recording.clone())),
            stdout: Box::pin(stdout),
            stderr,
            tree: Box::new(Tree),
        })))),
        release: ports.release,
        data: root.0.clone(),
        temporary: crate::account::Temporary::fixture(&root.0),
        tool_sets: mcp::ToolSets::new(0),
    };
    let runtime = Runtime::open(&start, root.0.join("unused-program"))
        .await
        .unwrap();
    Fixture {
        runtime,
        start,
        now,
        waits: waiting,
        recording,
        clock,
        _stdout: output,
        _root: root,
    }
}

#[tokio::test]
async fn stop_drains_more_than_a_channel_of_stderr_in_order() {
    // One cancel, 320 stderr lines under 64 KiB, five real seconds, 16 KiB of pipe bytes.
    tokio::time::timeout(Duration::from_secs(5), async {
        let expected: Vec<_> = (0..320)
            .map(|n| format!("https://example.com/{n}"))
            .collect();
        let text: String = expected
            .iter()
            .map(|url| format!("Open the following link to authenticate the ACP server: {url}\n"))
            .collect();
        assert!(text.len() < 64 * 1024);
        let (stderr, mut errors) = tokio::io::duplex(1);
        let mut f = fixture_with_stderr(Box::pin(stderr)).await;
        f.recording.allow(0);
        f.runtime.cancel("blocked").await.unwrap();
        f.waits.recv().await.unwrap();
        let recording = f.recording.clone();
        let draining = async {
            let events = f.runtime.stop_stderr().await;
            tokio::pin!(events);
            let mut actual = Vec::new();
            while let Some(event) = events.next().await {
                if let RuntimeEvent::SignInAddress(url) = event {
                    actual.push(url);
                }
            }
            actual
        };
        let unblock = async {
            errors.write_all(text.as_bytes()).await.unwrap();
            drop(errors);
            recording.allow(16 * 1024);
        };
        let (actual, ()) = tokio::join!(draining, unblock);
        assert_eq!(
            actual, expected,
            "stderr drained during join must precede stderr left in the channel"
        );
    })
    .await
    .expect("stderr join drain cap: 5 s");
}

#[tokio::test]
async fn stalled_cancel_releases_clock_then_hands_work_to_process_stop() {
    // One frame, five real seconds, 16 KiB of pipe bytes and 32 clock records.
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut f = fixture().await;
        f.recording.allow(0);
        f.runtime.tree = Box::new(GraceTree);
        f.runtime.cancel("blocked").await.unwrap();
        assert_eq!(f.waits.recv().await, Some(WRITE_TIMEOUT));
        let stopping = f.runtime.stop();
        let advancing = async {
            f.clock.advance_to(WRITE_TIMEOUT).await;
            assert_eq!(f.waits.recv().await, Some(35_000));
            assert_eq!(
                *f.clock.registrations.lock().unwrap(),
                [(WRITE_TIMEOUT, true), (35_000, true)],
                "cancel completion must retain work until the process deadline registers"
            );
            f.clock.advance_to(35_000).await;
        };
        tokio::join!(stopping, advancing);
        assert!(f.recording.bytes.lock().unwrap().is_empty());
        assert!(f.recording.closed.load(Ordering::SeqCst));
    })
    .await
    .expect("stalled cancel clock cap: 5 s");
}

#[tokio::test]
async fn stop_keeps_work_while_the_writer_join_is_pending() {
    // One frame, five real seconds, 16 KiB of pipe bytes and 32 clock records.
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut f = fixture().await;
        f.runtime
            .write(json!({"prompt":"blocked pipe content"}))
            .await
            .unwrap();
        f.waits.recv().await.unwrap();
        f.now.send_replace(30_200);
        let (waits, mut deadlines) = mpsc::unbounded_channel();
        let clock = Arc::new(TestClock {
            now: f.now.clone(),
            waits,
            holds: Arc::new(AtomicUsize::new(0)),
            registrations: Arc::default(),
        });
        f.runtime.clock = clock.clone();
        f.runtime.tree = Box::new(GraceTree);
        let stopping = f.runtime.stop();
        tokio::pin!(stopping);
        assert!(stopping.as_mut().now_or_never().is_none());
        assert!(
            clock.holds.load(Ordering::SeqCst) > 0,
            "the pending writer join must retain stop work"
        );
        tokio::select! {
            _ = &mut stopping => panic!("stop must wait for its process grace"),
            at = deadlines.recv() => assert_eq!(at, Some(35_200)),
        }
        assert_eq!(
            *clock.registrations.lock().unwrap(),
            [(35_200, true)],
            "the process deadline must exist before stop work ends"
        );
        assert_eq!(
            clock.holds.load(Ordering::SeqCst),
            0,
            "the process grace must permit clock advancement"
        );
        f.now.send_replace(35_200);
        stopping.await;
    })
    .await
    .expect("stop writer-join cap: 5 s");
}

#[tokio::test]
async fn stop_keeps_work_through_the_first_process_stop_poll() {
    // One frame, five real seconds, 16 KiB of pipe bytes and 32 clock records.
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut f = fixture().await;
        f.runtime
            .write(json!({"prompt":"a timed out partial frame"}))
            .await
            .unwrap();
        f.waits.recv().await.unwrap();
        f.now.send_replace(30_000);
        let _ = f.runtime.next().await;
        let _ = f.runtime.writer.join_next().await.unwrap().unwrap();
        let (waits, mut deadlines) = mpsc::unbounded_channel();
        let clock = Arc::new(TestClock {
            now: f.now.clone(),
            waits,
            holds: Arc::new(AtomicUsize::new(0)),
            registrations: Arc::default(),
        });
        f.runtime.clock = clock.clone();
        f.runtime.tree = Box::new(GraceTree);
        let stopping = f.runtime.stop();
        tokio::pin!(stopping);
        assert!(stopping.as_mut().now_or_never().is_none());
        assert_eq!(deadlines.recv().await, Some(35_000));
        assert_eq!(
            *clock.registrations.lock().unwrap(),
            [(35_000, true)],
            "the first stop poll must register its deadline under a hold"
        );
        assert_eq!(
            clock.holds.load(Ordering::SeqCst),
            0,
            "the grace itself must not retain stop work"
        );
        f.now.send_replace(35_000);
        stopping.await;
    })
    .await
    .expect("stop first-poll cap: 5 s");
}

#[tokio::test]
async fn stop_drains_queued_cancels_until_the_write_deadline() {
    // Four cancellation frames, five real seconds, at most 16 KiB per pipe.
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut sent = Vec::new();
        for receive in [true, false] {
            let mut f = fixture().await;
            f.recording.allow(0);
            f.runtime.cancel("first").await.unwrap();
            f.runtime.cancel("second").await.unwrap();
            f.waits.recv().await.unwrap();
            let recording = f.recording.clone();
            let mut stopping = std::pin::pin!(f.runtime.stop());
            assert!(futures_util::poll!(&mut stopping).is_pending());
            tokio::task::yield_now().await;
            if receive {
                recording.allow(16 * 1024);
            } else {
                f.now.send_replace(WRITE_TIMEOUT);
            }
            stopping.await;
            let bytes = recording.bytes.lock().unwrap().clone();
            sent.push(
                String::from_utf8(bytes)
                    .unwrap()
                    .lines()
                    .map(|line| serde_json::from_str::<Value>(line).unwrap())
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(
            sent,
            vec![
                vec![
                    json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"first"}}),
                    json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"second"}}),
                ],
                vec![],
            ],
            "FR-035/FR-045 send every queued cancel before stop unless its write deadline expires"
        );
    })
    .await
    .expect("cancel drain cap: 5 s");
}

#[tokio::test]
async fn stop_skips_a_prompt_queued_after_a_stalled_cancel() {
    // Two frames, five real seconds, at most 16 KiB per pipe.
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut f = fixture().await;
        f.recording.allow(0);
        f.runtime.tree = Box::new(GraceTree);
        f.runtime.cancel("running").await.unwrap();
        f.runtime
            .send("session/prompt", json!({"sessionId":"queued"}))
            .await
            .unwrap();
        assert_eq!(f.waits.recv().await, Some(WRITE_TIMEOUT));
        let stopping = f.runtime.stop();
        tokio::pin!(stopping);
        assert!(stopping.as_mut().now_or_never().is_none());
        f.recording.allow(16 * 1024);
        let advancing = async {
            while f.waits.recv().await != Some(5_000) {}
            f.clock.advance_to(5_000).await;
        };
        tokio::join!(stopping, advancing);
        assert_eq!(
            String::from_utf8(f.recording.bytes.lock().unwrap().clone()).unwrap(),
            format!(
                "{}\n",
                json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"running"}})
            ),
            "FR-035 forbids a queued prompt after stop starts"
        );
    })
    .await
    .expect("cancel then prompt cap: 5 s");
}

#[tokio::test]
async fn no_frame_follows_a_timed_out_partial_frame() {
    // Two frames, five real seconds, at most 16 KiB of recorded bytes.
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut f = fixture().await;
        f.runtime
            .write(json!({"first":"a frame stopped in the middle"}))
            .await
            .unwrap();
        f.waits.recv().await.unwrap();
        assert_eq!(&*f.recording.bytes.lock().unwrap(), b"{\"first\":\"a fram");
        f.runtime
            .write(json!({"second":"must never follow a partial first frame"}))
            .await
            .unwrap();
        f.now.send_replace(30_000);
        let _ = f.runtime.next().await;
        f.recording.allow(16 * 1024);
        f.runtime.stop().await;
        assert_eq!(
            &*f.recording.bytes.lock().unwrap(),
            b"{\"first\":\"a fram",
            "no frame may follow a timed-out partial frame"
        );
        assert!(f.recording.closed.load(Ordering::SeqCst));
    })
    .await
    .expect("recording pipe timeout cap: 5 s");
}

#[tokio::test]
async fn a_queued_frame_keeps_its_enqueue_deadline() {
    // Two frames, five real seconds, at most 16 KiB of recorded bytes.
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut f = fixture().await;
        let first = json!({"first":"complete after ten seconds"});
        let first_bytes = format!("{first}\n");
        f.runtime.write(first).await.unwrap();
        f.waits.recv().await.unwrap();
        f.runtime
            .write(json!({"second":"still blocked"}))
            .await
            .unwrap();
        f.now.send_replace(10_000);
        f.recording.allow(first_bytes.len());
        f.waits.recv().await.unwrap();
        assert_eq!(&*f.recording.bytes.lock().unwrap(), first_bytes.as_bytes());
        f.now.send_replace(30_000);
        let failed = tokio::time::timeout(Duration::from_millis(500), f.runtime.next())
            .await
            .is_ok();
        f.runtime.stop().await;
        assert!(
            failed,
            "a queued frame must fail at its enqueue deadline, not 30 s after dequeue"
        );
    })
    .await
    .expect("recording pipe queue deadline cap: 5 s");
}

#[tokio::test]
async fn the_writer_accepts_128_mib_and_rejects_the_next_frame() {
    // Six frames, ten real seconds, at most 16 KiB of recorded bytes.
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut f = fixture().await;
        f.runtime.write(json!(0)).await.unwrap();
        f.waits.recv().await.unwrap();
        assert_eq!(&*f.recording.bytes.lock().unwrap(), b"0\n");
        let length = 32 * 1024 * 1024 - 3; // JSON quotes and the newline complete 32 MiB.
        let pattern = "queue capacity boundary content; ";
        let mut text = pattern.repeat(length / pattern.len() + 1);
        text.truncate(length);
        assert_eq!(text.len(), length);
        f.runtime.write(json!(text)).await.unwrap();
        f.waits.recv().await.unwrap();
        for _ in 0..3 {
            f.runtime.write(json!(text)).await.unwrap();
        }
        let overflow = f.runtime.write(json!(0)).await;
        f.runtime.stop().await;
        assert!(matches!(overflow, Err(CommandError::Io(ref error)) if error.to_string() == "runtime stdin queue exceeded 134217728 bytes"), "128 MiB must be accepted, and the next frame must fail the runtime write");
        assert_eq!(f.recording.bytes.lock().unwrap().len(), 16);
        assert!(f.recording.closed.load(Ordering::SeqCst));
    }).await.expect("recording pipe capacity cap: 10 s");
}
