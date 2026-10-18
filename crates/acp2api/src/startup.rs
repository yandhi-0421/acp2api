use crate::*;
pub(crate) async fn start(
    mut ports: Ports,
    args: Vec<OsString>,
    env: Environment,
) -> Result<(), String> {
    if args.iter().any(|a| a == "--version") {
        println!("acp2api {}", ports.release.version());
        return Ok(());
    }
    if args.iter().any(|a| a == "--help") {
        println!("acp2api [--data-dir PATH] [--port PORT] [--help] [--version]");
        return Ok(());
    }
    let data = args
        .windows(2)
        .find(|p| p[0] == "--data-dir")
        .map(|p| PathBuf::from(&p[1]))
        .or_else(|| {
            env.home
                .as_ref()
                .map(|h| h.join("Library/Application Support/acp2api"))
        })
        .ok_or("missing data folder")?;
    let settings = crate::store::read(
        &data.join("config.toml"),
        ports.release.highest_concurrency_limit(),
    )?;
    let port = args
        .windows(2)
        .find(|p| p[0] == "--port")
        .map(|p| {
            p[1].to_str()
                .ok_or("port encoding")?
                .parse::<u16>()
                .map_err(|_| "port value")
        })
        .transpose()?
        .unwrap_or(settings.port);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|e| e.to_string())?;
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let secret: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let mcp_url = format!("http://127.0.0.1:{port}/mcp/{secret}");
    let runtime = crate::runtime_driver::Runtime::start(
        &ports,
        &data,
        env.temp.join("sessions/initial"),
        &mcp_url,
    )
    .await?;
    let (conductor, task) = crate::conductor::Conductor::start(runtime, ports.clock.clone());
    let router = crate::front::router(
        conductor,
        ports.clock.clone(),
        settings.local_api_key,
        secret,
    );
    println!("Listening http://127.0.0.1:{port}/");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            ports.stop.stopped().await;
        })
        .await
        .map_err(|e| e.to_string())?;
    task.await.map_err(|e| e.to_string())?;
    Ok(())
}
