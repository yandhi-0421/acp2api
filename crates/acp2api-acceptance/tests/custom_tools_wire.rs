use acp2api_acceptance::{Scenario, Setup, custom_tool_check as c, tool_check as t};
use serde_json::{Value, json};

#[tokio::test]
async fn custom_and_namespace_http_lever() {
    let input = if let Some(path) = std::env::var_os("ACP2API_CUSTOM_TOOL_PROBE_INPUT") {
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.len() <= 40 * 1024, "40 KiB input cap");
        serde_json::from_slice::<Value>(&bytes).unwrap()
    } else {
        json!({"custom":"patch_fresh","grammar":"start: /[a-z雪]+/\n// grammar 雪🙂","input":"*** Begin Patch\n+雪🙂\n*** End Patch\n","namespaces":["docs_fresh","images_fresh"],"member":"read_fresh","arguments":{"path":"fresh/雪🙂.txt","nested":[true,null,5]},"plain":"__acp2api_n_646f63735f6672657368_726561645f6672657368"})
    };
    for field in ["custom", "grammar", "input", "member", "plain"] {
        assert!(input[field].as_str().is_some_and(|s| s.len() <= 16384));
    }
    let namespaces = input["namespaces"].as_array().unwrap();
    assert_eq!(namespaces.len(), 2);
    assert_ne!(namespaces[0], namespaces[1]);
    let custom = input["custom"].as_str().unwrap();
    let grammar = input["grammar"].as_str().unwrap();
    let raw = input["input"].as_str().unwrap();
    let member = input["member"].as_str().unwrap();
    let plain = input["plain"].as_str().unwrap();
    let args = &input["arguments"];
    assert!(args.is_object());
    let mut evidence = Vec::new();
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let tools = vec![c::custom(custom, grammar)];
    for stream in [false, true] {
        let answer = c::send(&mut s, &tools, stream, Some(custom), &json!({"input":raw})).await;
        c::output(
            &answer,
            stream,
            "custom_tool_call",
            custom,
            None,
            &json!(raw),
        );
        evidence.push(capture(&answer, stream));
    }
    c::offer_custom(&s, custom, grammar);
    t::cancelled(&s, 2);
    // The name that resembles an encoded namespace member is a separate plain tool.
    let mut tools: Vec<Value> = namespaces
        .iter()
        .map(|n| c::namespace(n.as_str().unwrap(), &[member]))
        .collect();
    tools.push(json!({"type":"function","name":plain,"description":"Plain collision candidate","parameters":{"type":"object"}}));
    let answer = c::send(&mut s, &tools, false, None, &json!({})).await;
    assert_eq!(answer.status, 200, "namespace declarations accepted");
    let offers = c::offered(&s);
    assert_eq!(offers.len(), 3);
    let offered_names: Vec<_> = offers
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_owned())
        .collect();
    let unique: std::collections::HashSet<_> = offered_names.iter().collect();
    assert_eq!(
        unique.len(),
        3,
        "namespace/member/plain names cannot collide"
    );
    for index in [0, 1, 2, 0] {
        for stream in [false, true] {
            let answer = c::send(&mut s, &tools, stream, Some(&offered_names[index]), args).await;
            c::output(
                &answer,
                stream,
                "function_call",
                if index == 2 { plain } else { member },
                if index == 2 {
                    None
                } else {
                    namespaces[index].as_str()
                },
                args,
            );
            evidence.push(capture(&answer, stream));
        }
    }
    t::cancelled(&s, 10);
    let records = s.fake().record();
    let prompts = records.prompts();
    let sessions: std::collections::HashSet<_> = prompts.iter().map(|p| &p.session).collect();
    assert_eq!(
        sessions.len(),
        2,
        "tool sets are immutable and repeated calls retain identity"
    );
    let record = json!({"mcp_offers":records.offered_tools(),"permission_requests":t::permission_requests(&s),"permission_replies":t::permission_replies(&s),"cancels":records.cancels(),"responses":evidence});
    let output = record.to_string();
    assert!(output.len() <= 2 * 1024 * 1024, "2 MiB result cap");
    println!("CUSTOM_TOOL_PROBE {output}");
    s.finish(true).await;
}
fn capture(answer: &acp2api_acceptance::client::Answer, stream: bool) -> Value {
    json!({"stream":stream,"status":answer.status,"body":if stream{Value::Null}else{answer.json()},"events":answer.events.iter().map(|e|json!({"event":e.event,"data":e.data})).collect::<Vec<_>>()})
}
#[tokio::test]
async fn tool_choice_none_blocks_custom_and_namespace_permissions() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    for tool in [
        c::custom("off_patch", c::GRAMMAR),
        c::namespace("off_files", &["read"]),
    ] {
        let mut call = c::request(&[tool], false);
        call.body["tool_choice"] = json!("none");
        s.fake().next_prompt(t::plan(
            &["off_patch".into()],
            &json!({"input":"forbidden"}),
        ));
        let answer = t::send(&mut s, call).await;
        assert_eq!(answer.status, 200);
        assert_eq!(answer.json()["output"].as_array().unwrap().len(), 1);
        assert_eq!(answer.json()["output"][0]["type"], "message");
        t::offers(&s, &[]);
    }
    assert!(s.fake().record().cancels().is_empty());
    assert_eq!(t::permission_requests(&s).len(), 2);
    for reply in t::permission_replies(&s) {
        assert_eq!(reply["result"]["outcome"]["outcome"], "cancelled");
    }
    s.finish(true).await;
}

#[tokio::test]
async fn custom_namespace_keeps_both_kind_and_namespace() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let tools = vec![
        json!({"type":"namespace","name":"edits_mixed","description":"Edit tools","tools":[c::custom("patch_mixed",c::GRAMMAR)]}),
    ];
    let answer = c::send(&mut s, &tools, false, None, &json!({})).await;
    assert_eq!(answer.status, 200, "namespaced custom declaration accepted");
    let offers = c::offered(&s);
    let name = offers[0]["name"].as_str().unwrap();
    for stream in [false, true] {
        let answer = c::send(
            &mut s,
            &tools,
            stream,
            Some(name),
            &json!({"input":"精确\ninput🙂\n"}),
        )
        .await;
        c::output(
            &answer,
            stream,
            "custom_tool_call",
            "patch_mixed",
            Some("edits_mixed"),
            &json!("精确\ninput🙂\n"),
        );
    }
    t::cancelled(&s, 2);
    s.finish(true).await;
}
