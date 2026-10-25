use acp2api_acceptance::{Scenario, Setup, custom_tool_check as c, tool_check as t};
use serde_json::{Value, json};

#[tokio::test]
async fn namespace_context_reaches_mcp_and_preserves_member_identity() {
    let input = if let Some(path) = std::env::var_os("ACP2API_NAMESPACE_DESCRIPTION_INPUT") {
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.len() <= 40 * 1024, "40 KiB input cap");
        serde_json::from_slice::<Value>(&bytes).unwrap()
    } else {
        json!({
            "tools":[
                {"type":"namespace","name":"documents_context","description":"Documents only\n原文 雪🙂\n","tools":[
                    {"type":"function","name":"read_context","description":"Read document\n正文甲\n","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}},
                    {"type":"custom","name":"patch_context","description":"Apply patch\n正文乙\n","format":{"type":"grammar","syntax":"lark","definition":"start: /[^\\x00]+/\n// grammar 雪🙂\n"}}
                ]},
                {"type":"namespace","name":"images_context","description":"Images only\n原文 丙🙂\n","tools":[
                    {"type":"function","name":"read_context","description":"Read image\n正文丙\n","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}
                ]}
            ],
            "expected_descriptions":[
                "Namespace documents_context:\nDocuments only\n原文 雪🙂\n\n\nRead document\n正文甲\n",
                "Namespace documents_context:\nDocuments only\n原文 雪🙂\n\n\nApply patch\n正文乙\n\n\nGrammar (lark):\nstart: /[^\\x00]+/\n// grammar 雪🙂\n",
                "Namespace images_context:\nImages only\n原文 丙🙂\n\n\nRead image\n正文丙\n"
            ],
            "arguments":{"path":"context/雪🙂.txt","nested":[true,null,7]},
            "input":"*** Begin Patch\n+精确 雪🙂\n*** End Patch\n"
        })
    };
    let tools = input["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2, "two independent namespace contexts");
    assert_ne!(tools[0]["name"], tools[1]["name"]);
    assert_eq!(tools[0]["tools"].as_array().unwrap().len(), 2);
    assert_eq!(tools[1]["tools"].as_array().unwrap().len(), 1);
    let expected = input["expected_descriptions"].as_array().unwrap();
    assert_eq!(expected.len(), 3);
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let answer = c::send(&mut s, tools, false, None, &json!({})).await;
    assert_eq!(
        answer.status, 200,
        "namespace declaration accepted over HTTP"
    );
    let offers = c::offered(&s);
    println!("NAMESPACE_DESCRIPTION_OFFERS {}", json!(offers));
    assert_eq!(offers.len(), 3);
    let descriptions: Vec<Value> = offers
        .iter()
        .map(|offer| offer["description"].clone())
        .collect();
    assert_eq!(
        descriptions, *expected,
        "exact namespace and member context reaches actual MCP tools/list"
    );
    assert_eq!(offers[0]["inputSchema"], tools[0]["tools"][0]["parameters"]);
    assert_eq!(offers[2]["inputSchema"], tools[1]["tools"][0]["parameters"]);
    assert_eq!(
        offers[1]["inputSchema"],
        json!({"type":"object","properties":{"input":{"type":"string"}},"required":["input"],"additionalProperties":false})
    );
    let names: Vec<_> = offers
        .iter()
        .map(|offer| offer["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names.iter().collect::<std::collections::HashSet<_>>().len(),
        3
    );
    let mut responses = Vec::new();
    for (index, namespace_index, member_index) in [(0, 0, 0), (1, 0, 1), (2, 1, 0)] {
        let tool = &tools[namespace_index]["tools"][member_index];
        let custom = tool["type"] == "custom";
        let payload = if custom {
            json!({"input":input["input"]})
        } else {
            input["arguments"].clone()
        };
        for stream in [false, true] {
            let answer = c::send(&mut s, tools, stream, Some(names[index]), &payload).await;
            c::output(
                &answer,
                stream,
                if custom {
                    "custom_tool_call"
                } else {
                    "function_call"
                },
                tool["name"].as_str().unwrap(),
                tools[namespace_index]["name"].as_str(),
                if custom {
                    &input["input"]
                } else {
                    &input["arguments"]
                },
            );
            responses.push(json!({"stream":stream,"status":answer.status,"body":if stream {Value::Null} else {answer.json()},"events":answer.events.iter().map(|event|json!({"event":event.event,"data":event.data})).collect::<Vec<_>>()}));
        }
    }
    t::cancelled_tool_prompts(&s, 6, &[1, 2, 3, 4, 5, 6]);
    let records = s.fake().record();
    assert_eq!(
        records
            .prompts()
            .iter()
            .map(|prompt| &prompt.session)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        7,
        "seven separate one-user conversations use seven distinct ACP sessions"
    );
    let all_offers = records.offered_tools();
    assert_eq!(
        all_offers.len(),
        7,
        "each new session reads its immutable tool set"
    );
    for offering in all_offers {
        assert_eq!(
            offering["result"]["tools"],
            json!(offers),
            "every session retains the exact namespace mapping and declarations"
        );
    }
    let record = json!({"http_requests":7,"mcp_offers":records.offered_tools(),"permission_requests":t::permission_requests(&s),"permission_replies":t::permission_replies(&s),"cancels":records.cancels(),"responses":responses});
    let output = record.to_string();
    assert!(output.len() <= 2 * 1024 * 1024, "2 MiB result cap");
    println!("NAMESPACE_DESCRIPTION_PROBE {output}");
    s.finish(true).await;
}
