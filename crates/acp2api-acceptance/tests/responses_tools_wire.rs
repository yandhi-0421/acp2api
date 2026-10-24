use acp2api_acceptance::{Scenario, Setup, endpoint_check, runtime_safety_check, tool_check};
use serde_json::{Value, json};

#[tokio::test]
async fn response_objects_return_request_tool_settings_in_the_body_and_stream() {
    let tools = json!([
        {"type":"function","name":"lookup","description":"Look up a value 雪","parameters":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"],"additionalProperties":false},"strict":false},
        {"type":"custom","name":"patch","description":"Apply a patch","format":{"type":"grammar","syntax":"lark","definition":"start: /[a-z]+/"}},
        {"type":"namespace","name":"docs","description":"Document tools","tools":[{"type":"function","name":"read","parameters":{"type":"object","properties":{}}}]}
    ]);
    let mut observed = Vec::new();
    let mut expected = Vec::new();
    for (fields, expected_tools, expected_choice) in [
        (
            json!({"tools":tools,"tool_choice":"none"}),
            tools.clone(),
            "none",
        ),
        (
            json!({"tools":tools,"tool_choice":"auto"}),
            tools.clone(),
            "auto",
        ),
        (json!({"tools":tools}), tools.clone(), "auto"),
        (json!({}), json!([]), "auto"),
        (json!({"tools":null,"tool_choice":null}), json!([]), "auto"),
        (
            json!({"tools":tools,"tool_choice":{"type":"none"}}),
            tools.clone(),
            "none",
        ),
    ] {
        for (stream, failed) in [(false, false), (true, false), (true, true)] {
            let mut s = Scenario::default();
            s.plan(Setup::normal());
            if failed {
                s.fake()
                    .next_prompt(runtime_safety_check::execution_plan("wire_builtin"));
            }
            let mut call = endpoint_check::request("POST /v1/responses", "Hello", stream);
            call.body
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let answer = tool_check::send(&mut s, call).await;
            assert_eq!(answer.status, 200);
            let responses: Vec<_> = if stream {
                answer
                    .events
                    .iter()
                    .filter_map(|event| {
                        let value: Value = serde_json::from_str(&event.data).unwrap();
                        value
                            .get("response")
                            .map(|response| (event.event.clone(), response.clone()))
                    })
                    .collect()
            } else {
                vec![("body".into(), answer.json())]
            };
            observed.extend(responses.iter().map(|(event, response)| {
                json!([event, response["tools"], response["tool_choice"]])
            }));
            let events = if failed {
                vec![
                    "response.created",
                    "response.in_progress",
                    "response.failed",
                ]
            } else if stream {
                vec![
                    "response.created",
                    "response.in_progress",
                    "response.completed",
                ]
            } else {
                vec!["body"]
            };
            s.finish(true).await;
            expected.extend(
                events
                    .iter()
                    .map(|event| json!([event, expected_tools, expected_choice])),
            );
        }
    }
    assert_eq!(observed, expected);
}
