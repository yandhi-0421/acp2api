//! Explicit replies at the ACP boundary, independent of the default fake session behavior.
use crate::hub::State;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub enum RpcReply {
    Error(Value),
    Held {
        gate: String,
        reply: Box<RpcReply>,
    },
    Authentication {
        address: String,
        token: Option<String>,
        result: Option<Value>,
    },
    Silent,
    Reply {
        updates: Vec<Value>,
        result: Value,
    },
    FromRequest(fn(&[crate::hub::Entry], &Value) -> Value),
    FromCall(fn(&[crate::hub::Entry], &Value) -> RpcReply),
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
    let reply = if let RpcReply::FromCall(make) = reply {
        assert!(
            call.to_string().len() <= 64 * 1024,
            "scripted ACP input cap: 64 KiB"
        );
        let reply = make(&state.entries, call);
        assert!(
            format!("{reply:?}").len() <= 128 * 1024,
            "scripted ACP reply cap: 128 KiB"
        );
        reply
    } else {
        reply
    };
    let reply = match reply {
        RpcReply::Held { gate, reply } => {
            assert!(state.fake.held_replies.len() < 40, "held ACP call cap: 40");
            assert!(gate.len() <= 128, "ACP gate name cap: 128 bytes");
            state.record(json!({"kind":"rpc-held","gate":gate,"method":call["method"]}));
            assert!(
                state
                    .fake
                    .held_replies
                    .insert(gate, (label.into(), call.clone(), *reply))
                    .is_none()
            );
            return true;
        }
        RpcReply::Error(error) => {
            assert!(
                error.to_string().len() <= 64 * 1024,
                "ACP error cap: 64 KiB"
            );
            state.out(label, json!({"op":"Out","line":json!({"jsonrpc":"2.0","id":call["id"],"error":error}).to_string()}));
            return true;
        }
        RpcReply::Authentication {
            address,
            token,
            result,
        } => {
            assert_eq!(call["method"], "authenticate");
            assert!(address.len() <= 4096, "sign-in address cap: 4096 bytes");
            state.out(label, json!({"op":"Err","line":format!("Open the following link to authenticate the ACP server: {address}")}));
            let Some(result) = result else {
                return true;
            };
            if let Some(token) = token {
                assert!(token.len() <= 128, "synthetic token cap: 128 bytes");
                state.out(label, json!({"op":"KeepToken","token":token}));
            }
            state.fake.signed_in_after_authentication();
            RpcReply::Reply {
                updates: vec![],
                result,
            }
        }
        other => other,
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
