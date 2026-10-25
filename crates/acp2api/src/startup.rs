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
        crate::runtime_driver::session_root(&env)?,
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
