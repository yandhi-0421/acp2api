use acp2api_acceptance::{hub::Entry, rpc_script::RpcReply};
use serde_json::{Value, json};

// ACP payloads: Windows phase-s transcript, 2026-10-18; only the session ID is synthetic.
pub fn captured(field: &str) -> Value {
    serde_json::from_str::<Value>(include_str!("../fixtures/sign-in.json")).unwrap()[field].clone()
}

pub fn editor_home_reply(entries: &[Entry], _call: &Value) -> RpcReply {
    let home = entries
        .iter()
        .rev()
        .find_map(|e| e.event["hello"]["GEMINI_HOME"].as_str())
        .expect("fake process runtime home");
    // This marker belongs to the fake, and carries no account credential.
    if std::path::Path::new(home)
        .join("acp2api-synthetic-editor-sign-in")
        .is_file()
    {
        RpcReply::Reply {
            updates: vec![],
            result: captured("session"),
        }
    } else {
        RpcReply::Error(json!({"code":-32000,"message":"Authentication required"}))
    }
}
