fn main() -> std::process::ExitCode {
    let run = acp2api_acceptance::adapters::RunFile::read_from_env();
    if std::env::var_os("ACP2API_TEST_OUTBOUND_DIAL").is_some() {
        let _ = std::net::TcpStream::connect_timeout(
            &"192.0.2.1:9".parse().unwrap(),
            std::time::Duration::from_millis(200),
        );
    }
    acp2api::run(run.ports(), std::env::args_os(), run.environment)
}
