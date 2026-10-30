use acp2api::{Clock, HoldKind};
use acp2api_acceptance::{
    adapters::HarnessClock,
    client::ApiClient,
    hub::{Hub, HubLink},
    real_limit,
    setup::random_hex,
    timeline::TimeLine,
};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Notify, oneshot},
};

struct CompletionWake(Notify);
impl Wake for CompletionWake {
    fn wake(self: Arc<Self>) {
        self.0.notify_one();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.notify_one();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plain_http_completion_keeps_an_unrelated_deadline_in_the_future() {
    for advance in [true, false] {
        let hub = Hub::new(random_hex(24));
        let clock = HarnessClock::new(HubLink::new(&hub));
        let mut time = TimeLine::new(hub.clone());
        let timer = clock.sleep_until(120_000);
        let deadline = tokio::spawn(async move {
            let _hold = timer.await;
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let (seen, request_seen) = oneshot::channel();
        let (release, response_released) = oneshot::channel();
        let server = tokio::spawn(async move {
            tokio::time::timeout(real_limit(Duration::from_secs(30)), async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    assert!(head.len() < 8192, "fake HTTP request head cap: 8 KiB");
                    head.push(socket.read_u8().await.unwrap());
                }
                drop(clock.hold(HoldKind::HttpRequest));
                seen.send(()).unwrap();
                response_released.await.unwrap();
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
            }).await.expect("fake HTTP caps: one request, 30 s, no log");
        });
        let client = ApiClient::new(address, String::new(), hub.clone());
        let call = client.model_names();
        tokio::time::timeout(real_limit(Duration::from_secs(30)), request_seen)
            .await
            .unwrap()
            .unwrap();
        time.settle().await;
        assert_eq!(time.next_deadline(), Some(120_000));
        let initial_settle = HarnessClock::new(HubLink::new(&hub)).hold(HoldKind::TurnEvent);
        let notify = Arc::new(CompletionWake(Notify::new()));
        let waker = Waker::from(notify.clone());
        let mut completion = Box::pin(call.finish(&mut time));
        assert!(matches!(
            completion.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        ));
        drop(initial_settle);
        tokio::time::timeout(real_limit(Duration::from_secs(30)), notify.0.notified())
            .await
            .expect("completion's initial settle finishes");
        assert!(matches!(
            completion.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        ));
        release.send(()).unwrap();
        let answer = tokio::time::timeout(real_limit(Duration::from_secs(30)), completion)
            .await
            .expect("released HTTP response completes");
        let observed = hub.now();
        deadline.abort();
        let _ = deadline.await;
        server.await.unwrap();
        hub.close();
        assert_eq!(answer.status, 200);
        assert_eq!(answer.json(), serde_json::json!({}));
        assert_eq!(
            observed,
            if advance { 120_000 } else { 0 },
            "plain HTTP completion must not consume an unrelated virtual deadline"
        );
    }
}
