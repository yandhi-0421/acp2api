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
    let opened = crate::store::read(
        &data.join("config.toml"),
        ports.release.highest_concurrency_limit(),
    )?;
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
    std::fs::create_dir_all(data.join("logs")).map_err(|e| e.to_string())?;
    {
        use std::io::Write;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(data.join("logs/acp2api.log"))
            .map_err(|e| e.to_string())?;
        writeln!(log, "{start_line}").map_err(|e| e.to_string())?;
    }
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let secret: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let mcp_url = format!("http://127.0.0.1:{port}/mcp/{secret}");
    let program = data.join("official-runtime").join(if cfg!(windows) {
        "agy_acp_server.exe"
    } else {
        "agy_acp_server.par"
    });
    let runtime = if program.try_exists().map_err(|e| e.to_string())? {
        Some(
            crate::runtime_driver::Runtime::start(
                &ports,
                &data,
                env.temp.join("sessions/initial"),
                &mcp_url,
            )
            .await?,
        )
    } else {
        None
    };
    let (conductor, task) = crate::conductor::Conductor::start(runtime, ports.clock.clone());
    let router = crate::front::router(
        conductor,
        ports.clock.clone(),
        settings.local_api_key,
        secret,
    );
    if let Some(key) = opened.new_key {
        println!("acp2api: local API key: {key}");
    }
    if let Some(password) = opened.new_password {
        println!("acp2api: management password: {password}");
    }
    if opened.first_start {
        ports.browser.open(&panel);
    }
    println!("{start_line}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            ports.stop.stopped().await;
        })
        .await
        .map_err(|e| e.to_string())?;
    task.await.map_err(|e| e.to_string())?;
    Ok(())
}
