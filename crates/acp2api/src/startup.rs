use crate::*;
#[derive(Default)]
struct Options {
    data: Option<PathBuf>,
    port: Option<u16>,
    help: bool,
    version: bool,
}
fn options(args: Vec<OsString>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.into_iter().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help") if !options.help => options.help = true,
            Some("--version") if !options.version => options.version = true,
            Some("--data-dir") if options.data.is_none() => {
                let path = args
                    .next()
                    .filter(|s| !s.is_empty())
                    .ok_or("--data-dir requires a path")?;
                options.data = Some(path.into());
            }
            Some("--port") if options.port.is_none() => {
                let port = args
                    .next()
                    .and_then(|p| p.into_string().ok())
                    .and_then(|p| p.parse::<u16>().ok())
                    .filter(|&p| p > 0)
                    .ok_or("--port requires a port from 1 to 65535")?;
                options.port = Some(port);
            }
            _ => return Err("unknown option, command, or repeated option".into()),
        }
    }
    Ok(options)
}
fn deliver(opened: &crate::store::Opened) -> io::Result<()> {
    use std::io::Write;
    let mut terminal = std::io::stdout().lock();
    if let Some(key) = &opened.new_key {
        writeln!(terminal, "acp2api: local API key: {key}")?;
    }
    if let Some(password) = &opened.new_password {
        writeln!(terminal, "acp2api: management password: {password}")?;
    }
    terminal.flush()
}
pub(crate) async fn start(
    mut ports: Ports,
    args: Vec<OsString>,
    env: Environment,
) -> Result<(), String> {
    let options = options(args)?;
    if options.help {
        println!("acp2api [--data-dir PATH] [--port PORT] [--help] [--version]");
        return Ok(());
    }
    if options.version {
        println!("acp2api {}", ports.release.version());
        return Ok(());
    }
    let Some(mut account) =
        crate::account::Account::claim(&env, ports.clock.as_ref(), ports.stop.as_mut()).await?
    else {
        return Ok(());
    };
    let data = if let Some(data) = options.data {
        data
    } else if ports.release.platform().starts_with("windows-") {
        env.local_app_data
            .as_ref()
            .map(|p| p.join("acp2api"))
            .ok_or("missing LOCALAPPDATA for the data folder")?
    } else {
        env.home
            .as_ref()
            .map(|p| p.join("Library/Application Support/acp2api"))
            .ok_or("missing HOME for the data folder")?
    };
    let config = data.join("config.toml");
    let opened = crate::store::read(&config, ports.release.highest_concurrency_limit())?;
    deliver(&opened).map_err(|_| "cannot deliver generated credentials to the terminal")?;
    if opened.needs_save {
        crate::store::save(&config, &opened.settings)?;
    }
    let settings = opened.settings;
    let port = options.port.unwrap_or(settings.port);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AddrInUse {
                format!("the port {port} is in use. Set another port in the configuration file or with --port.")
            } else {
                format!("cannot listen on port {port}: {error}")
            }
        })?;
    let panel = format!("http://127.0.0.1:{port}/");
    let start_line = format!(
        "acp2api: {}: management panel at {panel}",
        ports.release.version()
    );
    let journal = crate::journal::Journal::prepare().map_err(|error| error.to_string())?;
    let tools = crate::runtime_driver::mcp::ToolSets::new(port);
    let builds = crate::runtime_builds::Builds::new(
        &ports,
        env.clone(),
        data.clone(),
        account.temporary.clone(),
        tools.clone(),
        port,
    );
    let (conductor, task) = crate::conductor::Conductor::start(
        None,
        ports.clock.clone(),
        journal.clone(),
        settings,
        data.join("config.toml"),
        ports.release.highest_concurrency_limit(),
    );
    let initial_conductor = conductor.clone();
    let initial_builds = builds.clone();
    let initial_clock = ports.clock.clone();
    let initial_browser = ports.browser.clone();
    let initial_panel = panel.clone();
    let initializing = async move {
        initial_conductor
            .builds(
                initial_builds,
                crate::runtime_builds::Action::Initial,
                initial_clock.hold(HoldKind::TurnEvent),
            )
            .await?;
        if opened.first_start {
            initial_browser.open(&initial_panel);
        }
        println!("{start_line}");
        Ok::<(), String>(())
    };
    let router = crate::front::router(
        conductor,
        ports.clock.clone(),
        tools,
        journal.clone(),
        port,
        builds,
    );
    let retained_listener = listener.into_std().map_err(|error| error.to_string())?;
    let listener = tokio::net::TcpListener::from_std(
        retained_listener
            .try_clone()
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    account.publish(&panel)?;
    journal
        .open_writer(&data.join("logs"), ports.clock.wall())
        .map_err(|error| error.to_string())?;
    account.temporary.recover()?;
    let clock = ports.clock.clone();
    let serving = async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                ports.stop.stopped().await;
            })
            .await
            .map_err(|error| error.to_string())
    };
    tokio::try_join!(initializing, serving)?;
    task.await.map_err(|e| e.to_string())?;
    account.finish(clock.as_ref(), retained_listener).await?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod lifecycle_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Stop(tokio::sync::oneshot::Receiver<()>);
    impl StopSignal for Stop {
        fn stopped(&mut self) -> BoxFuture<'_, ()> {
            Box::pin(async {
                let _ = (&mut self.0).await;
            })
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn recovery_runs_after_panel_publication_and_log_setup() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!("acp2api-recovery-order-{}", crate::random_hex::<12>().unwrap()));
            let previous = crate::account::Temporary::fixture(&root);
            let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
            let child = previous.launch(&crate::os::OsLauncher, clock.clone(), LaunchSpec {
                program: "/bin/cat".into(), args: vec![], env_set: vec![], cwd: previous.sessions(), own_temp: None,
            }).await.unwrap();
            let record_path = root.join("account/owned-temporary-files.json");
            let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&record_path).unwrap()).unwrap();
            record["roots"][0]["process"] = record["roots"][0]["processes"][0]["process"].clone();
            std::fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
            std::fs::create_dir(root.join("outside")).unwrap();
            std::os::unix::fs::symlink(root.join("outside"), previous.sessions().join("escape")).unwrap();
            drop(child.stdin);
            child.tree.stop(clock).await;
            let config = root.join("data/config.toml");
            let mut settings = crate::store::read(&config, 3).unwrap().settings;
            settings.port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
            crate::store::save(&config, &settings).unwrap();
            let env = Environment { account_root: root.join("account"), home: None, local_app_data: None, temp: root.join("temp"), path: vec![], exe_dir: root.join("exe") };
            let result = start(Ports::production(), vec!["acp2api".into(), "--data-dir".into(), root.join("data").into_os_string()], env).await;
            let actual = (result.is_err(), root.join("account/panel").is_file(), root.join("data/logs").is_dir());
            std::fs::remove_dir_all(root).unwrap();
            assert_eq!(actual, (true, true, true), "unsafe recovery must occur after publication and logs, before runtime initialization");
        }).await.expect("startup recovery-order cap: 5 seconds; one native child, no HTTP requests or captured process output");
    }

    #[tokio::test]
    async fn orderly_startup_exit_removes_only_its_registered_run() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-startup-owned-{}",
                crate::random_hex::<12>().unwrap()
            ));
            for folder in ["account", "temp", "data", "home", "exe"] {
                std::fs::create_dir_all(root.join(folder)).unwrap();
            }
            let mut settings = crate::store::read(&root.join("data/config.toml"), 3)
                .unwrap()
                .settings;
            let port = std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            settings.port = port;
            crate::store::save(&root.join("data/config.toml"), &settings).unwrap();
            let env = Environment {
                account_root: root.join("account"),
                home: Some(root.join("home")),
                local_app_data: None,
                temp: root.join("temp"),
                path: vec![],
                exe_dir: root.join("exe"),
            };
            let (send, recv) = tokio::sync::oneshot::channel();
            let mut ports = Ports::production();
            ports.stop = Box::new(Stop(recv));
            let task = tokio::spawn(start(
                ports,
                vec![
                    "acp2api".into(),
                    "--data-dir".into(),
                    root.join("data").into_os_string(),
                ],
                env,
            ));
            while !root.join("account/panel").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            let record: serde_json::Value = serde_json::from_slice(
                &std::fs::read(root.join("account/owned-temporary-files.json")).unwrap(),
            )
            .unwrap();
            let owned = PathBuf::from(record["roots"][0]["path"].as_str().unwrap());
            let layout = owned.join("sessions").is_dir() && owned.join("processes").is_dir();
            let unrelated = root.join("temp/ordinary-user-file");
            std::fs::write(&unrelated, b"keep").unwrap();
            let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            socket
                .write_all(
                    format!(
                        "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            let mut response = Vec::new();
            socket.take(65536).read_to_end(&mut response).await.unwrap();
            send.send(()).unwrap();
            task.await.unwrap().unwrap();
            let remaining: serde_json::Value = serde_json::from_slice(
                &std::fs::read(root.join("account/owned-temporary-files.json")).unwrap(),
            )
            .unwrap();
            let actual = (
                layout,
                !owned.exists(),
                remaining["roots"].as_array().unwrap().len(),
                std::fs::read(&unrelated).unwrap(),
            );
            std::fs::remove_dir_all(&root).unwrap();
            assert!(
                response.starts_with(b"HTTP/1.1 200"),
                "one panel request must reach the running host"
            );
            assert_eq!(
                actual,
                (true, true, 0, b"keep".to_vec()),
                "normal exit must remove its recorded run and preserve unrelated files"
            );
        })
        .await
        .expect("startup lifecycle cap: 5 seconds; one HTTP request, 64 KiB response cap");
    }
}
