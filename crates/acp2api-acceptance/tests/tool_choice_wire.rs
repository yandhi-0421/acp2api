use acp2api_acceptance::{Scenario, Setup, content_check as c, field_check as f, tool_check as t};
use serde_json::json;

#[tokio::test]
async fn tool_choice_allows_none_and_refuses_unrecognized_values_with_m02() {
    let endpoint = "POST /v1/chat/completions";
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut call = f::request(endpoint, false);
    call.body["tool_choice"] = json!({"type":"none"});
    c::send(&mut s, call).await;
    assert_eq!(s.answer.as_ref().unwrap().status, 200);
    t::offers(&s, &[]);
    s.finish(true).await;

    for (choice, label) in [
        (
            json!({"function":{"name":"read_file"}}),
            r#"tool_choice = {"function":{"name":"read_file"}}"#,
        ),
        (json!(5), "tool_choice = 5"),
    ] {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let mut call = f::request(endpoint, false);
        call.body["tool_choice"] = choice;
        c::send(&mut s, call).await;
        c::refusal(
            &s,
            endpoint,
            &format!("acp2api: the request field {label} is not supported."),
            "unsupported_parameter",
            Some("tool_choice"),
        );
        s.finish(true).await;
    }
}
