//! Explicit replies at the ACP boundary, independent of the default fake session behavior.
use crate::hub::State;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub enum RpcReply {
    Silent,
    Reply { updates: Vec<Value>, result: Value },
    FromRequest(fn(&[crate::hub::Entry], &Value) -> Value),
    Prompt(fn(&[crate::hub::Entry], &Value) -> crate::TurnPlan),
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
    let reply = if let RpcReply::FromRequest(make) = reply {
        assert!(
            call.to_string().len() <= 64 * 1024,
            "scripted ACP input cap: 64 KiB"
        );
        let result = make(&state.entries, call);
        assert!(
            result.to_string().len() <= 64 * 1024,
            "scripted ACP result cap: 64 KiB"
        );
        RpcReply::Reply {
            updates: vec![],
            result,
        }
    } else {
        reply
    };
    if let RpcReply::Prompt(make) = reply {
        assert_eq!(call["method"], "session/prompt");
        assert!(
            call.to_string().len() <= 64 * 1024,
            "scripted ACP input cap: 64 KiB"
        );
        let plan = make(&state.entries, call);
        assert!(
            plan.steps.len() <= 16 && format!("{plan:?}").len() <= 128 * 1024,
            "scripted ACP prompt cap: 16 steps, 128 KiB"
        );
        crate::fake::scripted_prompt(state, label, call, plan);
        return true;
    }
    if let RpcReply::Reply { updates, result } = reply {
        for update in updates {
            state.out(label, json!({"op":"Out","line":update.to_string()}));
        }
        let response = json!({"jsonrpc":"2.0","id":call["id"],"result":result});
        state.out(label, json!({"op":"Out","line":response.to_string()}));
    }
    true
}
