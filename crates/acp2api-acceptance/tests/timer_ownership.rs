#![allow(
    clippy::disallowed_methods,
    reason = "Real-time safety caps must work while virtual time is blocked"
)]
use acp2api::Clock;
use acp2api_acceptance::real_limit;
use acp2api_acceptance::{
    adapters::HarnessClock,
    hub::{Hub, HubLink},
    setup::random_hex,
    timeline::TimeLine,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_timer_releases_after_owner_finishes() {
    let hub = Hub::new(random_hex(24));
    let clock = HarnessClock::new(HubLink::new(&hub));
    let timer = clock.sleep_until(50);
    let task = tokio::spawn(async move {
        let _work = timer.await;
    });
    let mut time = TimeLine::new(hub.clone());
    let advancing = tokio::spawn(async move {
        time.advance_to(50).await;
    });
    task.await.unwrap();
    advancing.await.expect("finished timer owner must settle");
    assert!(hub.counts().balanced());
    hub.close();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_and_cancelled_timers_settle() {
    let hub = Hub::new(random_hex(24));
    let clock = HarnessClock::new(HubLink::new(&hub));
    drop(clock.sleep_until(1));
    let first = clock.sleep_until(50);
    let task = tokio::spawn(async move {
        let work = first.await;
        let second = clock.sleep_until(100);
        drop(work);
        let _work = second.await;
    });
    let mut time = TimeLine::new(hub.clone());
    time.advance_to(100).await;
    task.await.unwrap();
    assert!(hub.counts().balanced());
    assert_eq!(time.next_deadline(), None);
    hub.close();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn executing_callback_remains_visible_across_unrelated_timer_registration() {
    let hub = Hub::new(random_hex(24));
    let clock = HarnessClock::new(HubLink::new(&hub));
    let timer = clock.sleep_until(50);
    let (entered, callback_started) = tokio::sync::oneshot::channel();
    let (release, callback_release) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _work = timer.await;
        // A different owner registering a timer must not release this callback.
        drop(clock.sleep_until(500));
        entered.send(()).unwrap();
        callback_release.await.unwrap();
    });
    let mut time = TimeLine::new(hub.clone());
    let mut advancing = tokio::spawn(async move {
        time.advance_to(50).await;
    });
    tokio::time::timeout(
        real_limit(std::time::Duration::from_secs(3)),
        callback_started,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(hub.counts().wake_pending, 1);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut advancing)
            .await
            .is_err(),
        "settlement must wait for callback work"
    );
    release.send(()).unwrap();
    task.await.unwrap();
    advancing.await.unwrap();
    assert!(hub.counts().balanced());
    hub.close();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborting_awakened_owner_releases_its_work() {
    let hub = Hub::new(random_hex(24));
    let clock = HarnessClock::new(HubLink::new(&hub));
    let timer = clock.sleep_until(50);
    let (send, recv) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _work = timer.await;
        send.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    let mut time = TimeLine::new(hub.clone());
    let advance = tokio::spawn(async move {
        time.advance_to(50).await;
    });
    tokio::time::timeout(real_limit(std::time::Duration::from_secs(3)), recv)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hub.counts().wake_pending, 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    advance.await.unwrap();
    assert!(hub.counts().balanced());
    hub.close();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn os_process_stop_timer_finishes_without_followup_timer() {
    use acp2api::{LaunchSpec, ProcessLauncher};
    use acp2api_acceptance::{Scenario, Setup, adapters::GuardedLauncher};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    let s = Scenario::default();
    let run = s.root.seed(&s.hub, &Setup::normal());
    let launcher = GuardedLauncher::new(&run.fake_program, run.hub);
    let child = launcher
        .launch(LaunchSpec {
            program: s.root.path.join("data/official-runtime/agy_acp_server.par"),
            args: vec![],
            env_set: vec![],
            cwd: s.root.path.clone(),
            own_temp: None,
        })
        .unwrap();
    let clock = Arc::new(HarnessClock::new(HubLink::new(&s.hub)));
    let stopping = tokio::spawn(async move {
        child.tree.stop(clock).await;
    });
    let deadline = Instant::now() + real_limit(Duration::from_secs(3));
    while s.time.next_deadline() != Some(5000) {
        assert!(Instant::now() < deadline, "stop timer registration cap");
        let hub = s.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
    }
    let mut time = s.time.clone();
    time.advance_to(5000).await;
    stopping.await.unwrap();
    assert!(s.hub.counts().balanced());
    s.hub.close();
    s.root.cleanup();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_delivered_but_unpolled_timer_releases_its_wake() {
    use std::time::{Duration, Instant};
    let hub = Hub::new(random_hex(24));
    let clock = HarnessClock::new(HubLink::new(&hub));
    let timer = clock.sleep_until(50);
    let mut time = TimeLine::new(hub.clone());
    let advancing = tokio::spawn(async move {
        time.advance_to(50).await;
    });
    let deadline = Instant::now() + real_limit(Duration::from_secs(3));
    while hub.counts().wake_pending != 1 {
        assert!(Instant::now() < deadline, "timer delivery cap: 3 s");
        let hub = hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
    }
    drop(timer);
    advancing.await.unwrap();
    assert!(hub.counts().balanced());
    hub.close();
}
