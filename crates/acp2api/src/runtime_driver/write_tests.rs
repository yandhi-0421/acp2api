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
    bytes: Mutex<Vec<u8>>,
    limit: AtomicUsize,
    waker: AtomicWaker,
    closed: AtomicBool,
}
impl Recording {
    fn allow(&self, limit: usize) {
        self.limit.store(limit, Ordering::SeqCst);
        self.waker.wake();
    }
}
struct Pipe(Arc<Recording>);
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
struct TestClock {
    now: watch::Sender<Moment>,
    waits: mpsc::UnboundedSender<Moment>,
}
impl Clock for TestClock {
    fn now(&self) -> Moment {
        *self.now.borrow()
    }
    fn wall(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_millis(self.now())
    }
    fn hold(&self, _: HoldKind) -> Hold {
        Hold::empty()
    }
    fn sleep_until(&self, at: Moment) -> BoxFuture<'static, Hold> {
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
struct Launcher(Mutex<Option<Launched>>);
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
    _stdout: tokio::io::DuplexStream,
    _root: Root,
}
pub(crate) async fn fixture() -> Fixture {
    let root = Root(Environment::from_process().temp.join(format!(
        "acp2api-write-{}",
        crate::random_hex::<8>().unwrap()
    )));
    let (now, _) = watch::channel(0);
    let (waits, waiting) = mpsc::unbounded_channel();
    let recording = Arc::new(Recording {
        bytes: Mutex::new(Vec::new()),
        limit: AtomicUsize::new(16),
        waker: AtomicWaker::new(),
        closed: AtomicBool::new(false),
    });
    let (stdout, output) = tokio::io::duplex(1024);
    let ports = Ports::production();
    let start = RuntimeStart {
        stopped: watch::channel(false).1,
        clock: Arc::new(TestClock {
            now: now.clone(),
            waits,
        }),
        launcher: Arc::new(Launcher(Mutex::new(Some(Launched {
            stdin: Box::pin(Pipe(recording.clone())),
            stdout: Box::pin(stdout),
            stderr: Box::pin(tokio::io::empty()),
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
        _stdout: output,
        _root: root,
    }
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
