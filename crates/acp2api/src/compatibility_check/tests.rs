#![allow(clippy::disallowed_methods)]
use super::*;
use crate::{BoxFuture, Clock, Launched, Ports, ProcessLauncher, ProcessTree};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

struct Tree;
impl ProcessTree for Tree {
    fn pid(&self) -> u32 {
        std::process::id()
    }
    fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
        Box::pin(std::future::pending())
    }
    fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}
struct Launcher(Mutex<Option<Launched>>);
impl ProcessLauncher for Launcher {
    fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .take()
            .expect("one check process cap"))
    }
}
async fn listed_tools(url: &str) -> Value {
    let url = url.strip_prefix("http://").unwrap();
    let (address, path) = url.split_once('/').unwrap();
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let body = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string();
    socket.write_all(format!("POST /{path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    socket.take(16385).read_to_end(&mut bytes).await.unwrap();
    assert!(bytes.len() <= 16384, "MCP response cap: 16 KiB");
    let split = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap();
    serde_json::from_slice(&bytes[split + 4..]).unwrap()
}
async fn exercise(
    mut change: impl FnMut(&Value, &mut Vec<Value>, &Path) + Send + 'static,
) -> Result<String, Failure> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async move {
        let root = std::env::temp_dir().join(format!("acp2api-compatibility-fact-{}",crate::random_hex::<12>().unwrap()));
        std::fs::create_dir_all(root.join("home/.gemini")).unwrap();
        std::fs::write(root.join("home/.gemini/settings.json"), b"owner settings").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tools = crate::runtime_driver::mcp::ToolSets::new(listener.local_addr().unwrap().port());
        let server_tools = tools.clone();
        let server = tokio::spawn(async move { axum::serve(listener,server_tools.router()).await.unwrap() });
        let (input, output) = tokio::io::duplex(65536);
        let (stdout, agent_stdout) = tokio::io::duplex(65536);
        let agent_root = root.clone();
        let agent = tokio::spawn(async move {
            let mut input = BufReader::new(output);
            let mut stdout = agent_stdout;
            let mut system = String::new();
            let mut mcp = String::new();
            let mut prompts = 0;
            let mut prompt_id = Value::Null;
            let catalog = json!({"sessionId":"checked-session","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
            for _ in 0..32 {
                let mut line = String::new();
                if (&mut input).take(65537).read_line(&mut line).await.unwrap() == 0 { return; }
                assert!(line.len() <= 65536,"ACP request cap: 64 KiB");
                let call: Value = serde_json::from_str(&line).unwrap();
                let reply = |result| json!({"jsonrpc":"2.0","id":call["id"],"result":result});
                let text = |text: &str| json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"checked-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}});
                let mut replies = match call["method"].as_str() {
                    Some("initialize") => vec![reply(json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true,"mcpCapabilities":{"http":true},"_meta":{"antigravity":{"systemPrompt":{"append":true}}}},"authMethods":[{"id":"oauth-personal","name":"Google"}],"agentInfo":{"version":"1.6.0"}}))],
                    Some("session/new") => {
                        system = call["params"]["_meta"]["antigravity"]["systemPrompt"]["append"].as_str().unwrap().into();
                        mcp = call["params"]["mcpServers"][0]["url"].as_str().unwrap().into();
                        let folder = agent_root.join("data/runtime-home/antigravity-acp/conversations");
                        std::fs::create_dir_all(&folder).unwrap();
                        std::fs::write(folder.join("checked-session.db"), b"check session").unwrap();
                        vec![reply(catalog.clone())]
                    },
                    Some("session/load") => vec![reply(catalog.clone())],
                    Some("session/prompt") => {
                        prompts += 1;
                        match prompts {
                            1 => vec![text(&system),reply(json!({"stopReason":"end_turn"}))],
                            2 => {
                                prompt_id = call["id"].clone();
                                let name = listed_tools(&mcp).await["result"]["tools"][0]["name"].as_str().unwrap().to_owned();
                                vec![json!({"jsonrpc":"2.0","id":"permission","method":"session/request_permission","params":{"sessionId":"checked-session","toolCall":{"toolCallId":"tool","title":format!("Run {name}?"),"rawInput":{"value":"checked text"}},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"}]}})]
                            },
                            3 => vec![text(call["params"]["prompt"][0]["text"].as_str().unwrap()),reply(json!({"stopReason":"end_turn"}))],
                            _ => panic!("prompt cap: three"),
                        }
                    },
                    Some("session/cancel") => vec![json!({"jsonrpc":"2.0","id":prompt_id,"result":{"stopReason":"cancelled"}})],
                    None => Vec::new(),
                    method => panic!("unexpected ACP method: {method:?}"),
                };
                change(&call, &mut replies, &agent_root);
                for reply in replies {
                    let line = format!("{reply}\n");
                    assert!(line.len() <= 65536,"ACP reply cap: 64 KiB");
                    if stdout.write_all(line.as_bytes()).await.is_err() { return; }
                }
            }
            panic!("ACP request cap: 32");
        });
        let mut ports = Ports::production();
        ports.launcher = Arc::new(Launcher(Mutex::new(Some(Launched { stdin:Box::pin(input),stdout:Box::pin(stdout),stderr:Box::pin(tokio::io::empty()),tree:Box::new(Tree) }))));
        let start = RuntimeStart { clock:ports.clock,stopped:tokio::sync::watch::channel(false).1,launcher:ports.launcher,release:ports.release,data:root.join("data"),temporary:crate::account::Temporary::fixture(&root),tool_sets:tools };
        let result = run(&start,root.join("candidate"),Some(root.join("home/.gemini"))).await;
        agent.await.unwrap();
        server.abort();
        std::fs::remove_dir_all(root).unwrap();
        result
    }).await.expect("check fixture cap: 5 seconds; 32 ACP requests and 3 prompts; 64 KiB per message; one 16 KiB MCP response; no log")
}

#[tokio::test]
async fn all_compatibility_facts_pass() {
    let result = exercise(|_, _, _| {}).await;
    assert!(matches!(result,Ok(ref version) if version=="1.6.0"));
}

fn initialize(
    pointer: &'static str,
    value: Value,
) -> impl FnMut(&Value, &mut Vec<Value>, &Path) + Send {
    move |call, replies, _| {
        if call["method"] == "initialize" {
            *replies[0].pointer_mut(pointer).unwrap() = value.clone();
        }
    }
}
fn prompt(
    number: u8,
    change: impl Fn(&Value, &mut Vec<Value>) + Send,
) -> impl FnMut(&Value, &mut Vec<Value>, &Path) + Send {
    let mut seen = 0;
    move |call, replies, _| {
        if call["method"] == "session/prompt" {
            seen += 1;
            if seen == number {
                change(call, replies);
            }
        }
    }
}
fn incompatible(result: Result<String, Failure>) {
    assert!(
        matches!(result, Err(Failure::Incompatible(_))),
        "the broken fact must reject the compatibility check"
    );
}
#[tokio::test]
async fn rejects_missing_http_capability() {
    incompatible(
        exercise(initialize(
            "/result/agentCapabilities/mcpCapabilities/http",
            false.into(),
        ))
        .await,
    );
}
#[tokio::test]
async fn rejects_missing_system_append_capability() {
    incompatible(
        exercise(initialize(
            "/result/agentCapabilities/_meta/antigravity/systemPrompt/append",
            false.into(),
        ))
        .await,
    );
}
#[tokio::test]
async fn rejects_files_of_the_check_session_in_the_owners_folder() {
    for suffix in ["db", "db-wal", "db-shm", "meta"] {
        incompatible(
            exercise(move |call, _, root| {
                if call["method"] == "session/new" {
                    let folder = root.join("home/.gemini/antigravity-acp/conversations");
                    std::fs::create_dir_all(&folder).unwrap();
                    std::fs::write(
                        folder.join(format!("checked-session.{suffix}")),
                        b"wrong runtime home",
                    )
                    .unwrap();
                }
            })
            .await,
        );
    }
}
#[tokio::test]
async fn accepts_unrelated_writes_in_the_owners_folder() {
    let result = exercise(|call, _, root| {
        if call["method"] == "session/new" {
            for name in [
                "antigravity-acp/conversations/checked-session-other.db",
                "tmp/checked-session.db",
                ".DS_Store",
                "settings.json",
            ] {
                let path = root.join("home/.gemini").join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b"unrelated owner program").unwrap();
            }
        }
    })
    .await;
    assert!(
        matches!(result, Ok(ref version) if version == "1.6.0"),
        "unrelated owner-folder writes must not reject a compatible build"
    );
}

#[tokio::test]
async fn rejects_another_acp_version() {
    incompatible(exercise(initialize("/result/protocolVersion", 2.into())).await);
}
#[tokio::test]
async fn rejects_missing_session_load_support() {
    incompatible(
        exercise(initialize(
            "/result/agentCapabilities/loadSession",
            false.into(),
        ))
        .await,
    );
}
#[tokio::test]
async fn rejects_missing_personal_oauth() {
    incompatible(exercise(initialize("/result/authMethods", json!([]))).await);
}
#[tokio::test]
async fn rejects_missing_build_version() {
    incompatible(exercise(initialize("/result/agentInfo/version", Value::Null)).await);
}
#[tokio::test]
async fn rejects_a_reply_without_the_system_codeword() {
    incompatible(
        exercise(prompt(1, |_, replies| {
            replies[0]["params"]["update"]["content"]["text"] = "no codeword".into();
        }))
        .await,
    );
}
#[tokio::test]
async fn rejects_a_missing_client_tool_call() {
    incompatible(
        exercise(prompt(2, |call, replies| {
            *replies =
                vec![json!({"jsonrpc":"2.0","id":call["id"],"result":{"stopReason":"end_turn"}})];
        }))
        .await,
    );
}
#[tokio::test]
async fn rejects_a_built_in_tool_that_runs() {
    incompatible(exercise(prompt(1, |_, replies| {
        replies.insert(0,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"checked-session","update":{"sessionUpdate":"tool_call","toolCallId":"builtin","title":"view_file","status":"completed"}}}));
    })).await);
}
#[tokio::test]
async fn rejects_a_permission_without_the_run_title() {
    incompatible(
        exercise(prompt(2, |_, replies| {
            replies[0]["params"]["toolCall"]["title"] = "Unexpected title".into();
        }))
        .await,
    );
}
#[tokio::test]
async fn rejects_a_permission_without_object_arguments() {
    incompatible(
        exercise(prompt(2, |_, replies| {
            replies[0]["params"]["toolCall"]["rawInput"] = "invalid arguments".into();
        }))
        .await,
    );
}
#[tokio::test]
async fn rejects_a_prompt_that_does_not_end_as_cancelled() {
    incompatible(
        exercise(|call, replies, _| {
            if call["method"] == "session/cancel" {
                replies[0]["result"]["stopReason"] = "end_turn".into();
            }
        })
        .await,
    );
}
#[tokio::test]
async fn rejects_a_failed_session_load() {
    incompatible(
        exercise(|call, replies, _| {
            if call["method"] == "session/load" {
                replies[0].as_object_mut().unwrap().remove("result");
                replies[0]["error"] = json!({"code":-32002,"message":"Session not found"});
            }
        })
        .await,
    );
}
#[tokio::test]
async fn rejects_an_unexpected_event_during_load() {
    incompatible(exercise(|call,replies,_| {
        if call["method"] == "session/load" {
            replies.insert(0,json!({"jsonrpc":"2.0","id":"unexpected-permission","method":"session/request_permission","params":{}}));
        }
    }).await);
}
#[tokio::test]
async fn rejects_a_reply_without_the_textual_tool_result() {
    incompatible(
        exercise(prompt(3, |_, replies| {
            replies[0]["params"]["update"]["content"]["text"] = "no result".into();
        }))
        .await,
    );
}
#[tokio::test]
async fn rejects_an_acp_prompt_error() {
    incompatible(
        exercise(prompt(1, |_, replies| {
            replies[1].as_object_mut().unwrap().remove("result");
            replies[1]["error"] = json!({"code":-32000,"message":"prompt failed"});
        }))
        .await,
    );
}
#[tokio::test]
async fn rejects_an_invalid_permission_message() {
    incompatible(exercise(prompt(1, |_, replies| {
        replies.insert(0,json!({"jsonrpc":"2.0","id":"invalid-permission","method":"session/request_permission","params":{}}));
    })).await);
}

#[tokio::test]
async fn rejects_a_tool_outside_the_supplied_client_tools() {
    incompatible(
        exercise(prompt(2, |_, replies| {
            replies[0]["params"]["toolCall"]["title"] = "Run unknown_tool?".into();
        }))
        .await,
    );
}
#[tokio::test]
async fn rejects_tool_arguments_that_do_not_match_the_prompt() {
    incompatible(
        exercise(prompt(2, |_, replies| {
            replies[0]["params"]["toolCall"]["rawInput"] = json!({"value":"wrong text"});
        }))
        .await,
    );
}

#[tokio::test]
async fn rejects_a_missing_session_file_in_the_runtime_home() {
    incompatible(
        exercise(|call, _, root| {
            if call["method"] == "session/new" {
                std::fs::remove_file(
                    root.join("data/runtime-home/antigravity-acp/conversations/checked-session.db"),
                )
                .unwrap();
            }
        })
        .await,
    );
}
