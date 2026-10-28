//! Explicit replies at the ACP boundary, independent of the default fake session behavior.
use crate::hub::State;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub enum RpcReply {
    Silent,
    Reply { updates: Vec<Value>, result: Value },
}

pub(crate) fn respond(state: &mut State, label: &str, call: &Value) -> bool {
    let Some(reply) = state
        .fake
        .rpc_replies
        .get_mut(call["method"].as_str().unwrap_or(""))
        .and_then(|replies| replies.pop_front())
    else {
        return false;
    };
    if let RpcReply::Reply { updates, result } = reply {
        for update in updates {
            state.out(label, json!({"op":"Out","line":update.to_string()}));
        }
        let response = json!({"jsonrpc":"2.0","id":call["id"],"result":result});
        state.out(label, json!({"op":"Out","line":response.to_string()}));
    }
    true
}
