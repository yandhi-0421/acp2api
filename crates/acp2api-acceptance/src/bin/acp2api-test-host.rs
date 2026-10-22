fn main() -> std::process::ExitCode {
    let run = acp2api_acceptance::adapters::RunFile::read_from_env();
    acp2api::run(run.ports(), std::env::args_os(), run.environment)
}
