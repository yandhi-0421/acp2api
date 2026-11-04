fn main() -> std::process::ExitCode {
    let mut environment = acp2api::Environment::from_process();
    environment.account_root = std::env::var_os("ACP56_ACCOUNT_ROOT").unwrap().into();
    acp2api::run(acp2api::Ports::production(), std::env::args_os(), environment)
}
