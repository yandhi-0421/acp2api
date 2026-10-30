use crate::{
    World, panel_tiles_support as tiles, places_support as queue, setup_logs_support as check,
};
use acp2api_acceptance::{
    TurnPlan, content_check, endpoint_check, journal_check, rpc_script::RpcReply, system_check,
};
use cucumber::{given, then, when};
use serde_json::{Value, json};

#[given(expr = "acp2api answered {int} requests since its start")]
async fn answered(w: &mut World, count: usize) {
    assert_eq!(count, 51, "last-50 fixture cap: 51 requests");
    for _ in 0..count {
        check::send(w, "POST /v1/chat/completions", None).await;
    }
    check::no_secrets(w);
    // The panel must retain its own metadata when the disk logs no longer contain it.
    for file in journal_check::files(&w.scenario.root.path.join("data/logs")) {
        std::fs::write(file, "{\"request_id\":\"DISK-ONLY-REQUEST\"}\n").unwrap();
    }
}

#[then(expr = "the tile {string} shows {int} requests with their metadata")]
fn metadata(w: &mut World, title: String, count: usize) {
    check::no_secrets(w);
    let html = tiles::html(w);
    let tile = tiles::tile(&html, &title);
    assert!(
        !tile.contains("PRIVATE-SETUP-LOGS-")
            && !tile.contains(&w.scenario.root.key)
            && !tile.contains(&w.scenario.root.password),
        "FR-056 request metadata holds no message text or credentials (withheld)"
    );
    let rows = check::rows(tile);
    let header = rows.iter().find(|row| {
        row.iter()
            .any(|cell| check::normalized(cell) == "requestid")
    });
    assert!(
        header.is_some(),
        "FR-054 request metadata has a Request ID column"
    );
    let header = header.unwrap();
    let id_col = header
        .iter()
        .position(|cell| check::normalized(cell) == "requestid")
        .unwrap();
    let requests: Vec<_> = rows
        .iter()
        .filter(|row| row.get(id_col).is_some_and(|cell| cell.starts_with("req_")))
        .collect();
    assert_eq!(
        requests.len(),
        count,
        "FR-054 logs shows exactly the last 50 requests"
    );
    for expected in &w.setup_logs.requests[w.setup_logs.requests.len() - count..] {
        let row = requests
            .iter()
            .find(|row| row[id_col] == expected["request_id"].as_str().unwrap());
        assert!(
            row.is_some(),
            "FR-054 retained HTTP request ID appears in the logs tile"
        );
        let row = row.unwrap();
        for (key, expected) in expected.as_object().unwrap() {
            let column = header.iter().position(|label| {
                let label = check::normalized(label);
                label == check::normalized(key)
                    || matches!(
                        (key.as_str(), label.as_str()),
                        ("session", "acpsession") | ("session_use", "newcontinuerebuild")
                    )
            });
            assert!(
                column.is_some(),
                "FR-054 request metadata column missing: {key}"
            );
            let actual = &row[column.unwrap()];
            match expected {
                Value::Null => assert!(
                    ["", "—", "-", "null", "None"].contains(&actual.as_str()),
                    "FR-054 no error code on a successful request"
                ),
                Value::Array(fields) if fields.is_empty() => assert!(
                    ["", "—", "-", "[]", "None"].contains(&actual.as_str()),
                    "FR-054 request has no unknown fields"
                ),
                Value::String(value) if key == "api" => assert!(
                    [value.as_str(), "OpenAI Chat Completions API"].contains(&actual.as_str()),
                    "FR-054 request API"
                ),
                Value::String(value) => assert_eq!(actual, value, "FR-054 request metadata: {key}"),
                Value::Number(value) => assert_eq!(
                    actual.trim_end_matches(" ms"),
                    value.to_string(),
                    "FR-054 request metadata: {key}"
                ),
                _ => panic!("unexpected metadata fixture: {key}"),
            }
        }
    }
    assert!(
        !tiles::visible(tiles::tile(&html, &title)).contains("DISK-ONLY-REQUEST"),
        "FR-054 request rows come from memory"
    );
}

#[then(expr = "the tile {string} does not show the first request")]
fn oldest(w: &mut World, title: String) {
    let html = tiles::html(w);
    let text = tiles::visible(tiles::tile(&html, &title));
    let first = w.setup_logs.requests[0]["request_id"].as_str().unwrap();
    let last = w.setup_logs.requests[50]["request_id"].as_str().unwrap();
    assert!(
        !text.contains(first) && text.contains(last),
        "FR-054 first request is evicted and newest request remains"
    );
}

#[given(expr = "an API client sent {int} requests to {string} with the top-level field {string}")]
async fn unknown_requests(w: &mut World, count: usize, endpoint: String, field: String) {
    assert_eq!(count, 2, "unknown-field fixture cap: 2 requests");
    for _ in 0..count {
        check::send(w, &endpoint, Some((&field, json!(1)))).await;
    }
}

#[then(
    expr = "the table of the unknown field names has the row {string} with the API {string}, the count {int}, and the time of the last request"
)]
fn unknown_row(w: &mut World, name: String, api: String, count: usize) {
    check::unknown(w, &name, &api, count);
}

#[then(expr = "the tile {string} shows the log folder")]
fn log_folder(w: &mut World, title: String) {
    let html = tiles::html(w);
    let text = tiles::visible(tiles::tile(&html, &title));
    let path = w.scenario.root.path.join("data/logs");
    assert!(path.is_dir(), "real log folder exists");
    assert!(
        text.contains(path.to_str().unwrap()),
        "FR-054 logs tile shows the actual log folder"
    );
}

#[when(
    regex = r#"^an API client sends a valid request to "([^"]+)" with the top-level field "([^"]+)" set to (.+)$"#
)]
async fn unknown_request(w: &mut World, endpoint: String, field: String, value: String) {
    check::send(
        w,
        &endpoint,
        Some((&field, serde_json::from_str(&value).unwrap())),
    )
    .await;
}

#[then(expr = "the log has a line with the field name {string}")]
fn logged(w: &mut World, field: String) {
    check::no_secrets(w);
    let id = w.setup_logs.requests[0]["request_id"].as_str().unwrap();
    let records = journal_check::request_records(&w.scenario);
    let request: Vec<_> = records
        .iter()
        .filter(|record| record["request_id"] == id)
        .collect();
    assert_eq!(
        request.len(),
        1,
        "FR-014 one log record for this HTTP request"
    );
    assert_eq!(
        request[0]["unknown_fields"],
        json!([field]),
        "FR-014 log records the unknown field name"
    );
}

#[then(expr = "the management panel counts {string} one time for the OpenAI Chat Completions API")]
async fn counted(w: &mut World, field: String) {
    check::page(w).await;
    check::unknown(w, &field, "OpenAI Chat Completions API", 1);
}

#[given(expr = "the address of acp2api is {string}")]
fn configured_address(_w: &mut World, address: String) {
    assert_eq!(
        address, "http://127.0.0.1:8329",
        "8329 aliases the scenario's leased port"
    );
}

#[given(expr = "the official runtime reported the context size 1,048,576 for {string}")]
async fn context(w: &mut World, model: String) {
    assert_eq!(model, "gemini-3.8-flash");
    check::catalog(w);
    queue::start(w).await;
    w.request.updates.clear();
    w.request.plan = Some(TurnPlan::new().chunk("OK").usage(137, 1_048_576));
    w.request.queue_next(&w.scenario);
    content_check::send(
        &mut w.scenario,
        endpoint_check::request("POST /v1/chat/completions", "context size seed", false),
    )
    .await;
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, 200);
    assert_eq!(w.scenario.fake().record().prompts().len(), 1);
}

#[given("the official runtime reported no context size")]
async fn no_context(w: &mut World) {
    check::catalog(w);
    queue::start(w).await;
    assert!(
        w.scenario.fake().record().prompts().is_empty(),
        "no prompt has emitted a usage_update"
    );
}

#[then(expr = "the setup text for Claude Code has {string} set to {string}")]
fn claude_address(w: &mut World, name: String, address: String) {
    assert_eq!(address, "http://127.0.0.1:8329");
    assert_eq!(
        check::assignment(&check::setup(w), &name),
        format!("http://127.0.0.1:{}", w.scenario.root.port),
        "FR-054 Claude Code uses the actual address"
    );
}

#[then(
    "the setup text for Claude Code has a model name in \"ANTHROPIC_MODEL\", in each \"ANTHROPIC_DEFAULT_*_MODEL\", and in \"CLAUDE_CODE_SUBAGENT_MODEL\""
)]
fn claude_models(w: &mut World) {
    let setup = check::setup(w);
    for name in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "CLAUDE_CODE_SUBAGENT_MODEL",
    ] {
        check::model(check::assignment(&setup, name));
    }
}

#[then(expr = "the setup text for Claude Code has {string} set to {int}")]
fn claude_window(w: &mut World, name: String, size: u64) {
    assert_eq!(
        check::assignment(&check::setup(w), &name),
        size.to_string(),
        "FR-054 Claude Code uses the reported context window"
    );
}

#[then(
    "the setup text for Codex has a model name in \"model\", \"model_context_window\" set to 1048576, the base URL \"http://127.0.0.1:8329/v1\", and \"wire_api = \"responses\"\""
)]
fn codex(w: &mut World) {
    let blocks = check::snippets(w);
    let codex = blocks.iter().find(|block| block.contains("wire_api"));
    assert!(codex.is_some(), "FR-054 Codex setup text is present");
    let codex = codex.unwrap();
    check::model(check::assignment(codex, "model"));
    assert_eq!(check::assignment(codex, "model_context_window"), "1048576");
    assert_eq!(check::assignment(codex, "wire_api"), "responses");
    assert_eq!(
        check::assignment(codex, "base_url"),
        format!("http://127.0.0.1:{}/v1", w.scenario.root.port)
    );
}

#[then(expr = "the setup text for a Chat Completions client has the base URL {string}")]
fn chat(w: &mut World, address: String) {
    assert_eq!(address, "http://127.0.0.1:8329/v1");
    let blocks = check::snippets(w);
    let chat = blocks
        .iter()
        .find(|block| !block.contains("wire_api") && !block.contains("ANTHROPIC_BASE_URL"));
    assert!(
        chat.is_some(),
        "FR-054 separate Chat Completions setup text is present"
    );
    assert!(
        chat.unwrap()
            .contains(&format!("http://127.0.0.1:{}/v1", w.scenario.root.port)),
        "FR-054 Chat Completions base URL"
    );
}

#[then(
    "the setup text has a placeholder in each place of the local API key, also in \"ANTHROPIC_AUTH_TOKEN\""
)]
fn key_placeholders(w: &mut World) {
    let setup = check::setup(w);
    check::placeholder(
        check::assignment(&setup, "ANTHROPIC_AUTH_TOKEN"),
        "ANTHROPIC_AUTH_TOKEN",
    );
    for snippet in check::snippets(w) {
        let credentials: Vec<_> = snippet
            .lines()
            .filter_map(|line| {
                let (name, value) = line.split_once('=').or_else(|| line.split_once(':'))?;
                let name = name
                    .trim()
                    .trim_start_matches("export ")
                    .trim_matches('"')
                    .to_ascii_lowercase();
                [
                    "anthropic_auth_token",
                    "api_key",
                    "apikey",
                    "openai_api_key",
                    "experimental_bearer_token",
                    "authorization",
                ]
                .contains(&name.as_str())
                .then(|| {
                    value
                        .trim()
                        .trim_end_matches(',')
                        .trim_matches(['\'', '"'])
                        .trim_start_matches("Bearer ")
                })
            })
            .collect();
        assert!(
            !credentials.is_empty(),
            "FR-054 each client setup has an API-key placeholder"
        );
        for value in credentials {
            check::placeholder(value, "local API key");
        }
    }
}

#[then("the setup text does not hold the local API key")]
fn no_key(w: &mut World) {
    assert!(
        !check::setup(w).contains(&w.scenario.root.key),
        "FR-054 setup text holds no actual local API key (withheld)"
    );
    let html = tiles::html(w);
    assert!(
        html.contains(&w.scenario.root.key),
        "FR-054 the key is available separately from the setup text (withheld)"
    );
}

#[then(
    "\"CLAUDE_CODE_MAX_CONTEXT_TOKENS\" and \"model_context_window\" in the setup text hold a placeholder"
)]
fn window_placeholders(w: &mut World) {
    let setup = check::setup(w);
    for name in ["CLAUDE_CODE_MAX_CONTEXT_TOKENS", "model_context_window"] {
        check::placeholder(check::assignment(&setup, name), name);
    }
}

#[given("acp2api has not read the model names")]
async fn unread(w: &mut World) {
    // A failed initial sign-in check leaves the catalog unread. A later read can succeed.
    w.scenario.fake().script_call(
        "session/new",
        RpcReply::Error(json!({"code":-32000,"message":"Authentication required"})),
    );
    check::catalog(w);
    queue::start(w).await;
    w.setup_logs.sessions_before = w.scenario.fake().record().session_meta().len();
    assert_eq!(
        w.setup_logs.sessions_before, 1,
        "initial check attempted once"
    );
    assert!(w.scenario.fake().record().prompts().is_empty());
}

#[when(expr = "the owner uses the entry {string}")]
async fn read_models(w: &mut World, entry: String) {
    assert_eq!(entry, "Read the model names now");
    check::page(w).await;
    let html = tiles::html(w);
    let action = html.split("<form").skip(1).find_map(|part| {
        let form = part.split("</form>").next().unwrap();
        let button = form.split("<button").skip(1).any(|button| {
            tiles::visible(
                button
                    .split_once('>')
                    .unwrap()
                    .1
                    .split("</button>")
                    .next()
                    .unwrap(),
            ) == entry
        });
        button.then(|| {
            form.split("action=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned()
        })
    });
    if let Some(path) = action {
        w.scenario.fake().hold_next_session_result();
        w.setup_logs.read = Some(w.panel.owner("B1").submit(&path, json!({})));
        w.scenario.time.settle().await;
    }
}

#[then("the read holds one place and makes one ACP session")]
async fn read_place(w: &mut World) {
    assert!(
        w.setup_logs.read.is_some(),
        "FR-054 before the first model read, the panel offers Read the model names now"
    );
    assert_eq!(
        w.scenario.fake().record().session_meta().len(),
        w.setup_logs.sessions_before + 1,
        "FR-054 one ACP session for the owner model read"
    );
    let before = system_check::requests(&w.scenario).len();
    let answer = w.panel.page("B1").await;
    assert_eq!(
        system_check::requests(&w.scenario).len(),
        before,
        "FR-054 viewing the pending read sends no ACP request"
    );
    let html = acp2api_acceptance::panel_check::text(&answer);
    tiles::figure(tiles::sections(&html).next().unwrap(), "Places in use", "1");
    w.scenario.fake().release_session_result();
    let answer = w
        .setup_logs
        .read
        .take()
        .unwrap()
        .finish(&mut w.scenario.time)
        .await;
    assert!(
        [200, 303].contains(&answer.status),
        "FR-054 model-read action completes"
    );
    w.panel.answer = Some(w.panel.page("B1").await);
    let html = tiles::html(w);
    tiles::figure(tiles::sections(&html).next().unwrap(), "Places in use", "0");
    assert_eq!(
        w.scenario.fake().record().session_meta().len(),
        w.setup_logs.sessions_before + 1
    );
    assert!(
        w.scenario.fake().record().prompts().is_empty(),
        "FR-054 reading model names sends no prompt"
    );
}

#[then(expr = "the tile {string} shows the model names")]
fn model_names(w: &mut World, title: String) {
    check::no_secrets(w);
    let html = tiles::html(w);
    let text = tiles::visible(tiles::tile(&html, &title));
    for model in crate::model_names::NAMES {
        assert!(
            text.split_whitespace().any(|word| word == model),
            "FR-054 setup shows model name {model}"
        );
    }
}
