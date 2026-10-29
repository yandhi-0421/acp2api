#[path = "support/fake_pipe.rs"]
#[allow(
    dead_code,
    reason = "Sign-in contract uses only raw ACP operations of the shared pipe"
)]
mod fake_pipe;
use acp2api_acceptance::rpc_script::RpcReply;
use fake_pipe::FakePipe;
use serde_json::json;
#[path = "support/sign_in_facts.rs"]
mod sign_in_facts;
use sign_in_facts::captured;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn editor_sign_in_depends_on_the_home_the_real_fake_process_receives() {
    let mut pipe = FakePipe::new();
    let home = pipe.scenario.root.path.join("data/runtime-home");
    for id in 1..=2 {
        if id == 2 {
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(
                home.join("acp2api-synthetic-editor-sign-in"),
                "synthetic sign-in",
            )
            .unwrap();
        }
        pipe.scenario.fake().script_call(
            "session/new",
            RpcReply::FromCall(sign_in_facts::editor_home_reply),
        );
        pipe.send(json!({"jsonrpc":"2.0","id":id,"method":"session/new","params":{}}))
            .await;
        let response = pipe.stdout().await;
        if id == 1 {
            assert_eq!(response["error"]["message"], "Authentication required");
        } else {
            assert_eq!(response["result"]["sessionId"], "synthetic-sign-in-session");
        }
    }
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn captured_authentication_changes_the_sign_in_and_only_the_runtime_keeps_the_token() {
    let mut pipe = FakePipe::new();
    pipe.scenario.fake().set_signed_in(false);
    pipe.scenario.fake().script_call(
        "initialize",
        RpcReply::Reply {
            updates: vec![],
            result: captured("initialize"),
        },
    );
    pipe.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    assert_eq!(
        pipe.stdout().await["result"]["authMethods"][0]["id"],
        "oauth-personal"
    );
    pipe.send(json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{}}))
        .await;
    assert_eq!(
        pipe.stdout().await["error"],
        json!({"code":-32000,"message":"Authentication required"})
    );
    pipe.scenario.fake().script_call(
        "authenticate",
        RpcReply::Authentication {
            address: "https://accounts.example.com/signin".into(),
            token: Some("T1".into()),
            result: Some(captured("authenticated")),
        },
    );
    pipe.send(json!({"jsonrpc":"2.0","id":3,"method":"authenticate","params":{"methodId":"oauth-personal"}})).await;
    assert_eq!(
        pipe.stderr().await,
        "Open the following link to authenticate the ACP server: https://accounts.example.com/signin"
    );
    assert_eq!(
        pipe.stdout().await,
        json!({"jsonrpc":"2.0","id":3,"result":captured("authenticated")})
    );
    pipe.send(json!({"jsonrpc":"2.0","id":4,"method":"session/new","params":{}}))
        .await;
    assert_eq!(pipe.stdout().await["result"]["sessionId"], "session-1");
    assert_eq!(
        std::fs::read_to_string(
            pipe.scenario
                .root
                .path
                .join("data/runtime-home/synthetic-token")
        )
        .unwrap(),
        "T1"
    );
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_check_releases_its_own_error_without_consuming_the_next_reply() {
    let mut pipe = FakePipe::new();
    pipe.scenario.fake().script_call(
        "session/new",
        RpcReply::Held {
            gate: "check-A".into(),
            reply: Box::new(RpcReply::Error(
                json!({"code":-32000,"message":"Authentication required"}),
            )),
        },
    );
    pipe.send(json!({"jsonrpc":"2.0","id":1,"method":"session/new","params":{}}))
        .await;
    pipe.send(json!({"jsonrpc":"2.0","id":2,"method":"initialize"}))
        .await;
    assert_eq!(
        pipe.stdout().await["id"],
        2,
        "the held check has no reply before release"
    );
    pipe.scenario.fake().release_call("check-A");
    assert_eq!(
        pipe.stdout().await,
        json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"Authentication required"}})
    );
    pipe.send(json!({"jsonrpc":"2.0","id":3,"method":"session/new","params":{}}))
        .await;
    assert_eq!(pipe.stdout().await["result"]["sessionId"], "session-1");
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unfinished_authentication_emits_the_captured_address_line_without_a_result() {
    let mut pipe = FakePipe::new();
    pipe.scenario.fake().set_signed_in(false);
    pipe.scenario.fake().script_call(
        "authenticate",
        RpcReply::Authentication {
            address: "https://accounts.example.com/signin".into(),
            token: None,
            result: None,
        },
    );
    pipe.send(json!({"jsonrpc":"2.0","id":1,"method":"authenticate","params":{"methodId":"oauth-personal"}})).await;
    assert_eq!(
        pipe.stderr().await,
        "Open the following link to authenticate the ACP server: https://accounts.example.com/signin"
    );
    pipe.send(json!({"jsonrpc":"2.0","id":2,"method":"initialize"}))
        .await;
    assert_eq!(
        pipe.stdout().await["id"],
        2,
        "the authentication result remains outstanding"
    );
    assert!(!pipe.scenario.fake().signed_in());
    pipe.finish().await;
}
