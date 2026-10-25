use super::World;
use cucumber::{given, then, when};
use toml::Value;
#[given(regex = r#"^(?:the data folder|the folder) "([^"]+)" is empty$"#)]
async fn empty(w: &mut World, name: String) {
    w.startup.initialize().await;
    w.startup.data = w.startup.root().path.join(name);
    std::fs::create_dir_all(&w.startup.data).unwrap();
    assert_eq!(std::fs::read_dir(&w.startup.data).unwrap().count(), 0);
}
#[given(expr = "the data folders {string} and {string} are empty")]
async fn two_empty(w: &mut World, a: String, b: String) {
    empty(w, a.clone()).await;
    let b = w.startup.root().path.join(b);
    std::fs::create_dir_all(&b).unwrap();
    assert_eq!(std::fs::read_dir(b).unwrap().count(), 0);
}
#[given(expr = "a computer with {word} and no data folder of acp2api")]
async fn platform_folder(w: &mut World, platform: String) {
    w.startup.initialize().await;
    w.startup.run.as_mut().unwrap().release.platform = if platform == "Windows" {
        "windows-x64"
    } else {
        "macos-arm64"
    }
    .into();
    w.startup.data = if platform == "Windows" {
        w.startup.root().path.join("local-app-data/acp2api")
    } else {
        w.startup
            .root()
            .path
            .join("home/Library/Application Support/acp2api")
    };
    assert!(!w.startup.data.exists());
}
#[given(expr = "the platform maximum concurrency limit is {int}")]
#[given(
    expr = "the build of acp2api for this platform has the highest concurrency limit {int}, as its release notes name"
)]
async fn maximum(w: &mut World, n: u8) {
    w.startup.initialize().await;
    w.startup.run.as_mut().unwrap().release.highest = n;
}
#[given(expr = "the configuration file has the port {int}")]
async fn port(w: &mut World, n: i64) {
    w.startup.initialize().await;
    w.startup.set("port", Some(Value::Integer(n)));
}
#[given(expr = "the configuration file has the concurrency limit {int}")]
async fn concurrency(w: &mut World, n: i64) {
    w.startup.initialize().await;
    w.startup.set("concurrency_limit", Some(Value::Integer(n)));
}
#[given(expr = "acp2api is stopped")]
async fn stopped(w: &mut World) {
    w.startup.initialize().await;
    w.startup.seed_config();
}
#[given(expr = "the owner sets the port {int} in the configuration file")]
async fn edit_port(w: &mut World, n: i64) {
    w.startup.set("port", Some(Value::Integer(n)));
}
#[given(expr = "the configuration file has no concurrency limit")]
async fn absent_limit(w: &mut World) {
    w.startup.initialize().await;
    w.startup.set("concurrency_limit", None);
}
#[given(regex = r#"^the configuration file has no field for the (port|concurrency limit)$"#)]
async fn absent(w: &mut World, name: String) {
    w.startup.initialize().await;
    w.startup.set(&name.replace(' ', "_"), None);
}
#[given(expr = "acp2api made the configuration file")]
async fn made(w: &mut World) {
    w.startup.initialize().await;
    w.startup.launch_data(vec![]).await;
}
#[given(expr = "another program listens on the port {int}")]
async fn busy(w: &mut World, p: u16) {
    w.startup.occupied = Some(
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, p))
            .expect("exclusive port fixture"),
    );
}
#[given(expr = "the configuration file has the unknown key {string} with the value {int}")]
async fn unknown(w: &mut World, k: String, n: i64) {
    w.startup.initialize().await;
    w.startup.set(&k, Some(Value::Integer(n)));
}
#[given(expr = "the configuration file has these lines:")]
async fn syntax(w: &mut World, step: &cucumber::gherkin::Step) {
    w.startup.initialize().await;
    std::fs::write(
        w.startup.config_path(),
        step.docstring.as_deref().unwrap().trim_matches('\n'),
    )
    .unwrap();
}
#[when(expr = "the owner runs {string}")]
async fn cli(w: &mut World, command: String) {
    w.startup.initialize().await;
    w.startup
        .launch(
            command
                .split_whitespace()
                .skip(1)
                .map(str::to_owned)
                .collect(),
        )
        .await;
}
#[when(expr = "the owner starts acp2api with no option")]
async fn no_option(w: &mut World) {
    w.startup.launch(vec![]).await;
}
#[when(expr = "the owner starts acp2api with the data folder {string}")]
async fn data(w: &mut World, name: String) {
    assert_eq!(w.startup.data, w.startup.root().path.join(name));
    assert_eq!(
        std::fs::read_dir(&w.startup.data).unwrap().count(),
        0,
        "first-start folder must remain empty until launch"
    );
    w.startup.launch_data(vec![]).await;
}
#[when(
    expr = "the owner does a first start with the data folder {string} and then a first start with the data folder {string}"
)]
async fn two_starts(w: &mut World, a: String, b: String) {
    for name in [a, b] {
        w.startup.data = w.startup.root().path.join(name);
        assert_eq!(std::fs::read_dir(&w.startup.data).unwrap().count(), 0);
        w.startup.launch_data(vec![]).await;
        w.startup.remember_secrets();
        w.startup.stop().await;
    }
}
#[when(expr = "the owner starts acp2api with the option {string}")]
async fn cli_option(w: &mut World, option: String) {
    let args = option
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if option.starts_with("--data-dir") {
        w.startup.launch(args).await;
    } else {
        w.startup.launch_data(args).await;
    }
}
#[when(expr = "the owner starts acp2api")]
#[when(expr = "the owner starts acp2api with no option that overrides that setting")]
async fn start(w: &mut World) {
    w.startup.launch_data(vec![]).await;
}
#[then(expr = "the help text lists the options {string}, {string}, {string}, and {string}")]
fn help(w: &mut World, a: String, b: String, c: String, d: String) {
    let text = w.startup.terminal();
    for item in [a, b, c, d] {
        assert!(text.contains(&item), "help must list {item}");
    }
}
#[then(expr = "the help text lists no other option and no command")]
fn only_help(w: &mut World) {
    let text = w.startup.terminal();
    assert!(
        w.startup.exited == Some(true),
        "help must exit successfully"
    );
    let words = text.split_whitespace().collect::<Vec<_>>();
    assert!(
        words.iter().all(|v| matches!(
            *v,
            "acp2api" | "[--data-dir" | "PATH]" | "[--port" | "PORT]" | "[--help]" | "[--version]"
        )),
        "help exposes an unexpected command or option"
    );
}
#[then(expr = "the browser opens the address of the management panel")]
fn browser(w: &mut World) {
    assert_eq!(
        w.startup.browser_urls(),
        vec!["http://127.0.0.1:8329/".to_owned()],
        "first start must invoke browser port with panel address"
    );
}
#[then(expr = "the two local API keys are different")]
fn keys(w: &mut World) {
    assert!(
        w.startup.keys.len() == 2 && w.startup.keys[0] != w.startup.keys[1],
        "first starts must generate different API keys (withheld)"
    );
}
#[then(expr = "the two management passwords are different")]
fn passwords(w: &mut World) {
    assert!(
        w.startup.passwords.len() == 2 && w.startup.passwords[0] != w.startup.passwords[1],
        "first starts must generate different passwords (withheld)"
    );
}
#[then(regex = r#"^the configuration file "([^"]+)" and the logs are in the folder "(.*)"$"#)]
fn location(w: &mut World, file: String, folder: String) {
    let intended = match folder.as_str() {
        "~/Library/Application Support/acp2api/" => w
            .startup
            .root()
            .path
            .join("home/Library/Application Support/acp2api"),
        "%LOCALAPPDATA%\\acp2api\\" => w.startup.root().path.join("local-app-data/acp2api"),
        _ => w.startup.root().path.join(folder),
    };
    assert_eq!(
        w.startup.data, intended,
        "Gherkin path must retain its intended value"
    );
    assert!(
        intended.join(file).is_file(),
        "configuration file must exist in selected data folder"
    );
    assert!(
        intended.join("logs").is_dir(),
        "logs must exist in selected data folder"
    );
}
#[then(expr = "the data folder of the platform holds no file of acp2api")]
fn platform_empty(w: &mut World) {
    for p in [
        "home/Library/Application Support/acp2api",
        "local-app-data/acp2api",
    ] {
        assert!(
            !w.startup.root().path.join(p).exists(),
            "explicit folder must leave platform folders untouched"
        );
    }
}
#[then(expr = "the configuration file has the port {int} and the concurrency limit {int}")]
fn defaults(w: &mut World, p: i64, n: i64) {
    let value = w.startup.saved();
    assert!(
        value.is_some(),
        "first start must write valid configuration"
    );
    let value = value.unwrap();
    assert_eq!(value["port"].as_integer(), Some(p));
    assert_eq!(value["concurrency_limit"].as_integer(), Some(n));
}
#[then(expr = "the configuration file has no path of agy_acp_server")]
fn no_path(w: &mut World) {
    assert!(
        w.startup
            .saved()
            .unwrap()
            .get("agy_acp_server_path")
            .is_none()
    );
}
#[then(expr = "the configuration file has the concurrency limit {int}")]
fn limit(w: &mut World, n: i64) {
    let value = w.startup.saved().unwrap();
    assert_eq!(
        value.get("concurrency_limit").and_then(Value::as_integer),
        Some(n),
        "absent concurrency must be saved as platform default"
    );
}
#[then(regex = r#"^acp2api uses the (port|concurrency limit) (\d+)$"#)]
async fn uses(w: &mut World, name: String, n: i64) {
    assert!(
        w.startup.exited.is_none(),
        "valid default settings must keep listener available without runtime"
    );
    if name == "port" {
        assert!(
            w.startup.responds(n as u16).await,
            "default port must answer requests"
        );
    } else {
        limit(w, n);
    }
}
#[then(expr = "acp2api answers requests on the port {int}")]
async fn answers(w: &mut World, p: u16) {
    assert!(
        w.startup.responds(p).await,
        "configured port must return an HTTP response"
    );
}
#[then(expr = "acp2api accepts no connection on the port {int}")]
async fn no_connection(w: &mut World, p: u16) {
    assert!(
        !w.startup.responds(p).await,
        "overridden port must not accept requests"
    );
}
#[then(
    expr = "the file permissions of the configuration file give no other computer account read access"
)]
fn permissions(w: &mut World) {
    assert!(
        w.startup.config_path().is_file(),
        "first start must create configuration"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(w.startup.config_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "configuration must be owner-only"
        );
    }
    #[cfg(windows)]
    acp2api_acceptance::startup_permissions::assert_owner_only(&w.startup.config_path());
}
#[then(expr = "the start fails with the message {string}")]
fn failure(w: &mut World, message: String) {
    assert!(
        w.startup.exited == Some(false),
        "unusable configuration/port must fail startup"
    );
    let expected = message.replace("<setting>", "concurrency_limit").replace(
        "<path of the configuration file>",
        w.startup.config_path().to_str().unwrap(),
    );
    assert!(
        w.startup.terminal().lines().any(|l| l == expected),
        "startup diagnostic must match M-25/M-26 with key/location and path (terminal withheld)"
    );
}
#[then(
    expr = "the message names the setting of the concurrency limit and the path of the configuration file"
)]
fn error_location(w: &mut World) {
    let text = w.startup.terminal();
    assert!(
        text.contains("concurrency_limit")
            && text.contains(w.startup.config_path().to_str().unwrap()),
        "diagnostic must name concurrency_limit and configuration path"
    );
}
#[then(expr = "acp2api accepts no API request")]
#[then(expr = "acp2api listens on no other port")]
fn no_listener(w: &mut World) {
    assert!(
        w.startup.exited == Some(false),
        "failed start must exit without a listener"
    );
}
