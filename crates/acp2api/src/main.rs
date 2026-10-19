fn main() -> std::process::ExitCode {
    acp2api::run(
        acp2api::Ports::production(),
        std::env::args_os(),
        acp2api::Environment::from_process(),
    )
}
