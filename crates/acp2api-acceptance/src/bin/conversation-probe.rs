use acp2api_acceptance::{Scenario, Setup, conversation_check as c, journal_check};
use serde_json::{Value, json};
#[tokio::main]
async fn main() {
    let input = std::env::args()
        .nth(1)
        .expect("JSON steps argument required");
    assert!(input.len() <= 65536, "input cap 64 KiB");
    let steps: Value = serde_json::from_str(&input).expect("JSON steps");
    let steps = steps.as_array().expect("array of steps");
    assert!(!steps.is_empty() && steps.len() <= 24, "1..24 requests");
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut observations = Vec::new();
    for step in steps {
        if step["restart"] == true {
            s.restart().await;
        }
        let mut call = c::call(
            step["messages"].clone(),
            step["system"].as_str(),
            step["tools"].clone(),
        );
        if let Some(headers) = step["headers"].as_object() {
            for (key, value) in headers {
                call.headers
                    .push((key.clone(), value.as_str().expect("header text").into()));
            }
        }
        if let Some(key) = step.get("prompt_cache_key") {
            call.body["prompt_cache_key"] = key.clone();
        }
        c::send(
            &mut s,
            call,
            step["reply"].as_str().expect("reply text required"),
        )
        .await;
        observations.push(json!({"session":c::session(&s),"prompt":c::blocks(&s),"reply":s.answer.as_ref().unwrap().json()["choices"][0]["message"]["content"],"kind":c::kind(&s),"log":journal_check::records(&s).last().unwrap()}));
    }
    let out = json!({"observations":observations,"virtual_ms":s.hub.now(),"session_meta":s.fake().record().session_meta(),"offerings":s.fake().record().offered_tools()});
    assert!(out.to_string().len() < 1048576, "output cap 1 MiB");
    println!("{out}");
    s.finish(true).await;
}
