use acp2api_acceptance::{Scenario, Setup, content_check as c, system_check as s};
use serde_json::{Value, json};
#[tokio::test]
async fn parent_system_probe() {
    let input = if let Some(path) = std::env::var_os("ACP2API_SYSTEM_PROBE_INPUT") {
        let bytes = std::fs::read(path).expect("probe input file");
        assert!(bytes.len() <= 32 * 1024, "probe input cap: 32 KiB");
        serde_json::from_slice::<Value>(&bytes).expect("probe input JSON")
    } else {
        json!({"part_one":"First fresh rule 雪\ninside","part_two":"Second fresh rule 🙂","system_name":"系统 雪","developer_name":"规则 🙂","user":" Raw fresh user 雪🙂\nline ","attribution":"x-anthropic-billing-header: synthetic fresh probe attribution"})
    };
    let field = |key: &str, limit: usize| {
        let text = input[key].as_str().expect("probe text field");
        assert!(
            !text.is_empty() && text.len() <= limit,
            "probe field size cap"
        );
        text
    };
    let first = field("part_one", 8192);
    let second = field("part_two", 8192);
    let system_name = field("system_name", 256);
    let developer_name = field("developer_name", 256);
    let user = field("user", 8192);
    let attribution = field("attribution", 2048);
    assert!(attribution.starts_with("x-anthropic-billing-header:"));
    assert!(
        [first, second, system_name, developer_name, user]
            .iter()
            .all(|text| !text.contains(attribution)),
        "attribution must be distinct from kept input"
    );
    let mut scenario = Scenario::default();
    scenario.plan(Setup::normal());
    let mut facts = Vec::new();
    for endpoint in [
        "POST /v1/messages",
        "POST /v1/chat/completions",
        "POST /v1/responses",
    ] {
        let messages = json!([
            s::message("system", Some(system_name), first),
            s::message("developer", Some(developer_name), second),
            s::message("user", None, user)
        ]);
        let (system, messages, expected) = if endpoint.ends_with("messages") {
            (
                json!([{"type":"text","text":attribution},{"type":"text","text":first},{"type":"text","text":second}]),
                Value::Null,
                format!("{first}\n\n{second}"),
            )
        } else {
            (
                Value::Null,
                messages,
                format!(
                    "[system: {system_name}]\n{first}\n\n[developer: {developer_name}]\n{second}"
                ),
            )
        };
        for stream in [false, true] {
            let mut call = s::request(endpoint, system.clone(), messages.clone(), user);
            call.body["stream"] = stream.into();
            s::send(&mut scenario, call).await;
            s::addition(&scenario, &expected);
            s::prompt(&scenario, user);
            s::no_attribution(&scenario, attribution);
            let requests = s::requests(&scenario);
            let new = requests
                .iter()
                .rev()
                .find(|v| v["method"] == "session/new")
                .unwrap();
            let prompt = c::blocks(&scenario).pop().unwrap();
            facts.push(json!({"endpoint":endpoint,"stream":stream,"status":scenario.answer.as_ref().unwrap().status,"session_new_meta":new["params"]["_meta"],"prompt":prompt,"attribution_absent_from_all_acp_request_fields":true}));
        }
    }
    assert_eq!(
        scenario.fake().record().prompts().len(),
        6,
        "probe request cap: six API calls"
    );
    let output = json!({"observations":facts}).to_string();
    assert!(output.len() <= 256 * 1024, "probe output cap: 256 KiB");
    println!("SYSTEM_PROBE {output}");
    scenario.finish(true).await;
}
