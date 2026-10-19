use acp2api_acceptance::{Scenario, Setup, TurnPlan, tool_check as t};
use serde_json::{Value, json};
#[tokio::test]
async fn parent_tool_probe() {
    let input = if let Some(path) = std::env::var_os("ACP2API_TOOL_PROBE_INPUT") {
        let bytes = std::fs::read(path).expect("probe input file");
        assert!(bytes.len() <= 40 * 1024, "probe input cap: 40 KiB");
        serde_json::from_slice::<Value>(&bytes).expect("probe input JSON")
    } else {
        json!({"name":"parent_fresh_probe","description":"Fresh parent tool","schema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]},"arguments":{"path":"fresh/雪🙂.txt","flag":true,"nested":[null,7]},"choice":"auto"})
    };
    let name = input["name"].as_str().unwrap();
    assert!(
        !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "probe tool name"
    );
    let schema = &input["schema"];
    let args = &input["arguments"];
    assert!(schema.is_object() && args.is_object());
    assert!(
        schema.to_string().len() <= 16384 && args.to_string().len() <= 16384,
        "schema and argument cap: 16 KiB each"
    );
    let description = input["description"].as_str().unwrap();
    assert!(description.len() <= 2048);
    let choice = input["choice"].as_str().unwrap();
    assert!(matches!(choice, "auto" | "none"));
    let tool = json!({"name":name,"description":description,"inputSchema":schema});
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut outputs = Vec::new();
    for endpoint in [
        "POST /v1/messages",
        "POST /v1/chat/completions",
        "POST /v1/responses",
    ] {
        for stream in [false, true] {
            s.fake().next_prompt(if choice == "auto" {
                t::plan(&[name.into()], args)
            } else {
                TurnPlan::default()
            });
            let answer = t::send(
                &mut s,
                t::request(endpoint, std::slice::from_ref(&tool), stream, choice),
            )
            .await;
            if choice == "auto" {
                if stream {
                    t::stream(&answer, endpoint, name, args);
                } else {
                    t::body(&answer, endpoint, name, args);
                }
            } else {
                acp2api_acceptance::endpoint_check::verify(&answer, endpoint, "OK", 1000, 1);
            }
            outputs.push(json!({"endpoint":endpoint,"stream":stream,"status":answer.status,"body":if stream{Value::Null}else{answer.json()},"events":answer.events.iter().map(|e|json!({"event":e.event,"data":e.data})).collect::<Vec<_>>()}));
        }
    }
    if choice == "auto" {
        t::offers(&s, std::slice::from_ref(&tool));
        t::cancelled(&s, 6);
    } else {
        t::offers(&s, &[]);
        assert!(s.fake().record().cancels().is_empty());
    }
    let record = s.fake().record();
    let evidence = json!({"mcp_offers":record.offered_tools(),"responses":outputs,"cancels":record.cancels(),"permission_requests":t::permission_requests(&s),"permission_replies":t::permission_replies(&s),"mcp_execution_count":s.hub.entries().iter().filter(|e|e.event["request"]["method"]=="tools/call").count()});
    let output = evidence.to_string();
    assert!(output.len() <= 2 * 1024 * 1024, "probe output cap: 2 MiB");
    println!("TOOL_PROBE {output}");
    s.finish(true).await;
}
