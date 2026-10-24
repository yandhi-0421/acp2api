use crate::real_limit;
use crate::{
    Scenario, Setup, TurnPlan, TurnStep,
    client::{Answer, ApiCall},
    panel_check::Panel,
    tool_check,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};

pub fn isolated() -> Setup {
    Setup::normal().configure(|root, _, run| {
        let owner = root.path.join("home/.gemini");
        for folder in [
            "config/skills/owner-skill",
            "antigravity-cli/skills/owner-cli-skill",
        ] {
            std::fs::create_dir_all(owner.join(folder)).unwrap();
            std::fs::write(owner.join(folder).join("SKILL.md"), "synthetic skill").unwrap();
        }
        std::fs::write(
            owner.join("config/mcp_config.json"),
            r#"{"mcpServers":{"owner-mcp":{"command":"synthetic-only"}}}"#,
        )
        .unwrap();
        std::fs::write(
            owner.join("config/hooks.json"),
            r#"{"hooks":["owner-hook"]}"#,
        )
        .unwrap();
        let ordinary = root.path.join("temp/ordinary-project");
        std::fs::create_dir_all(&ordinary).unwrap();
        std::fs::write(ordinary.join("keep"), "ordinary user directory").unwrap();
        let fake = crate::setup::binary("safety-acp-agent");
        let name = if cfg!(windows) {
            "agy_acp_server.exe"
        } else {
            "agy_acp_server.par"
        };
        let program = root.path.join("data/official-runtime").join(name);
        std::fs::copy(&fake, &program).unwrap();
        run.fake_program = fake;
        run.release.builds[0].agy_acp_server_sha256 =
            Sha256::digest(std::fs::read(&program).unwrap())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
        let descriptor = program.parent().unwrap().join(if cfg!(windows) {
            "localharness_external.exe"
        } else {
            "localharness_external"
        });
        let mut text = std::fs::read_to_string(&descriptor).unwrap();
        text.push_str(&format!(
            "owner_home = {:?}\nline_cap = 300\nrun_ms = 180000\n",
            root.path.join("home").to_str().unwrap()
        ));
        std::fs::write(&descriptor, &text).unwrap();
        run.release.builds[0].localharness_external_sha256 = Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
    })
}
pub fn observations(s: &Scenario) -> Vec<Value> {
    s.hub
        .entries()
        .into_iter()
        .filter_map(|e| e.event["hello"].get("observation").cloned())
        .collect()
}
pub fn frames(s: &Scenario, method: &str) -> Vec<Value> {
    s.hub
        .entries()
        .into_iter()
        .filter(|e| e.event["kind"] == "got")
        .filter_map(|e| serde_json::from_str::<Value>(e.event["line"].as_str()?).ok())
        .filter(|v| v["method"] == method)
        .collect()
}
pub fn home(s: &Scenario) {
    let hellos: Vec<_> = s
        .hub
        .entries()
        .into_iter()
        .filter(|e| e.event["hello"]["role"] == "server")
        .collect();
    assert_eq!(hellos.len(), 1, "one actual guarded runtime launch");
    let hello = &hellos[0].event["hello"];
    assert_eq!(
        PathBuf::from(hello["GEMINI_HOME"].as_str().unwrap()),
        s.root.path.join("data/runtime-home")
    );
    assert!(
        hello["environment_names"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v != "AGY_ACP_FORCE_FILE_STORAGE")
    );
    assert_eq!(
        s.hub
            .entries()
            .iter()
            .filter(|e| e.event["kind"] == "launch" && e.event["allowed"] == true)
            .count(),
        1
    );
}
pub fn configuration(s: &Scenario) {
    let observed = observations(s);
    assert!(!observed.is_empty(), "fake observed actual session inputs");
    for observation in observed {
        assert_eq!(
            observation["configuration"],
            json!({"mcp":[],"hooks":[],"skills":[]}),
            "owner global MCP, hooks and skills must not reach an ACP session"
        );
    }
    tool_check::offers(s, &[]);
    assert!(
        s.root
            .path
            .join("home/.gemini/config/skills/owner-skill/SKILL.md")
            .is_file()
    );
    assert_eq!(
        std::fs::read_to_string(s.root.path.join("temp/ordinary-project/keep")).unwrap(),
        "ordinary user directory"
    );
}
pub fn capabilities(s: &Scenario) {
    let initialize = frames(s, "initialize");
    assert_eq!(initialize.len(), 1, "actual initialize delivered");
    assert_eq!(initialize[0]["params"]["clientInfo"]["name"], "acp2api");
    let capabilities = initialize[0]["params"]["clientCapabilities"]
        .as_object()
        .unwrap();
    assert!(!capabilities.contains_key("fs"), "no filesystem capability");
    assert!(
        !capabilities.contains_key("terminal"),
        "no terminal capability"
    );
    let sessions = frames(s, "session/new");
    assert!(!sessions.is_empty(), "actual new session delivered");
    for session in sessions {
        assert_eq!(
            session["params"]["_meta"]["antigravity"]["enabledTools"],
            json!([])
        );
        assert_eq!(
            session["params"]["_meta"]["antigravity"]["eagerMcpServers"],
            json!(["client-tools"])
        );
    }
}
pub fn denial_plan(name: &str, reply: &str) -> TurnPlan {
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::BuiltInPermission {
        title: format!("Run {name}?"),
    });
    plan.after(Duration::from_millis(10))
        .chunk(reply)
        .usage(1000, 1_048_576)
        .end("end_turn")
}
pub fn execution_plan(name: &str) -> TurnPlan {
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::SafetyToolReport {
        id: "safety-event".into(),
        title: Some(name.into()),
        status: "in_progress".into(),
        update: false,
    });
    plan.steps.push(TurnStep::SafetyToolReport {
        id: "safety-event".into(),
        title: Some(name.into()),
        status: "completed".into(),
        update: true,
    });
    plan.after(Duration::from_millis(20))
        .chunk("unsafe reply must not escape")
        .end("end_turn")
}
pub async fn send(s: &mut Scenario, text: &str) -> Answer {
    s.start_isolated_owner().await;
    tool_check::send(s, ApiCall::one_user_message("POST /v1/messages", text)).await
}
pub fn denied(s: &Scenario) {
    let requests = tool_check::permission_requests(s);
    assert_eq!(
        requests.len(),
        1,
        "one built-in permission request delivered"
    );
    let replies: Vec<_> = tool_check::permission_replies(s)
        .into_iter()
        .filter(|v| v["id"] == requests[0]["id"])
        .collect();
    assert_eq!(replies.len(), 1, "one actual refusal delivered");
    assert_eq!(
        replies[0]["result"]["outcome"],
        json!({"outcome":"cancelled"})
    );
    assert!(
        s.fake().record().cancels().is_empty(),
        "built-in permission refusal must let the prompt continue"
    );
    assert_eq!(
        s.fake().record().prompts().len(),
        1,
        "ordinary reply follows actual prompt"
    );
}
pub fn reply(answer: &Answer, text: &str) {
    assert_eq!(answer.status, 200);
    assert_eq!(
        answer.json()["content"],
        json!([{"type":"text","text":text}])
    );
    assert_eq!(answer.json()["stop_reason"], "end_turn");
}
pub fn error(answer: &Answer, name: &str) {
    assert_eq!(
        answer.status, 502,
        "unpermitted built-in execution must fail"
    );
    assert_eq!(
        answer.json(),
        json!({"type":"error","error":{"type":"api_error","message":format!("acp2api: the built-in tool {name} of the official runtime ran with no permission request, so acp2api cancelled the prompt turn.")}})
    );
}
pub fn log_records(s: &Scenario) -> Vec<Value> {
    let mut records = Vec::new();
    for file in std::fs::read_dir(s.root.path.join("data/logs")).unwrap() {
        let file = file.unwrap();
        assert!(
            file.metadata().unwrap().len() < 4 * 1024 * 1024,
            "per-log test cap: 4 MiB"
        );
        for line in std::fs::read_to_string(file.path()).unwrap().lines() {
            records.push(serde_json::from_str(line).expect("actual log JSON"));
        }
    }
    records
}
pub fn event_log(s: &Scenario, kind: &str, name: &str) {
    let records = log_records(s);
    let matches: Vec<_> = records.iter().filter(|v| v["event"] == kind).collect();
    assert_eq!(matches.len(), 1, "one real safety-event log report");
    assert_eq!(matches[0]["tool"], name);
    assert!(
        matches[0]["request_id"]
            .as_str()
            .is_some_and(|v| v.starts_with("req_"))
    );
    assert!(
        matches[0]["session"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    let completion: Vec<_> = records.iter().filter(|v| v.get("api").is_some()).collect();
    assert_eq!(
        completion.len(),
        1,
        "one completion separate from the safety event"
    );
    assert_eq!(completion[0]["request_id"], matches[0]["request_id"]);
    if kind == "built_in_tool_ran" {
        assert_eq!(completion[0]["result"], "failed");
        assert_eq!(completion[0]["error_code"], "built_in_tool_ran");
    } else {
        assert_eq!(completion[0]["result"], "success");
    }
}
pub async fn panel(s: &mut Scenario) -> Panel {
    let mut panel = Panel::default();
    panel.scenario(s).await;
    panel.login_initial("owner").await;
    assert_eq!(panel.login_answer.as_ref().unwrap().status, 303);
    panel
}
pub fn notice(name: &str) -> String {
    format!(
        "The built-in tool {name} ran with no permission request. acp2api cancelled the prompt turn and sent an error to the API client."
    )
}
pub async fn notice_page(panel: &mut Panel, expected: &str) {
    let page = panel.page("owner").await;
    assert_eq!(page.status, 200);
    let html = crate::panel_check::text(&page);
    assert_eq!(
        html.matches(expected).count(),
        1,
        "one persistent notice in authenticated panel"
    );
    assert!(
        html.contains("/manage/notice"),
        "notice has an actual dismissal form"
    );
}
pub async fn dismiss(panel: &mut Panel) {
    let page = panel.page("owner").await;
    let html = crate::panel_check::text(&page);
    let marker = "name=\"notice_id\" value=\"";
    let id = html
        .split_once(marker)
        .expect("dismissal notice ID")
        .1
        .split('"')
        .next()
        .unwrap();
    assert!(!id.is_empty(), "literal nonempty dismissal control");
    let answer = panel
        .submit("owner", "/manage/notice", json!({"notice_id":id}))
        .await;
    assert_eq!(answer.status, 303, "real notice dismissal succeeds");
}
pub async fn no_notice(panel: &mut Panel, expected: &str) {
    for _ in 0..2 {
        let page = panel.page("owner").await;
        let html = crate::panel_check::text(&page);
        assert_eq!(page.status, 200);
        assert!(
            html.contains("id=\"local-api-key\""),
            "authenticated page remains usable"
        );
        assert!(
            !html.contains(expected),
            "notice stays dismissed after reload"
        );
        assert!(!html.contains("name=\"notice_id\""));
    }
}
pub async fn two_conversations(s: &mut Scenario) {
    s.start_isolated_owner().await;
    for gate in ["safety-one", "safety-two"] {
        let mut plan = TurnPlan::new();
        plan.steps.push(TurnStep::HoldUntil(gate.into()));
        s.fake().next_prompt(plan.chunk(gate));
    }
    let client = s.client().await.clone();
    let first = client.send(ApiCall::one_user_message(
        "POST /v1/messages",
        "first distinct conversation",
    ));
    wait_prompts(s, 1).await;
    let second = client.send(ApiCall::one_user_message(
        "POST /v1/messages",
        "second distinct conversation",
    ));
    wait_prompts(s, 2).await;
    s.fake().open_gate("safety-one");
    s.fake().open_gate("safety-two");
    reply(&first.finish(&mut s.time).await, "safety-one");
    reply(&second.finish(&mut s.time).await, "safety-two");
}
pub async fn wait_prompts(s: &mut Scenario, count: usize) {
    let deadline = std::time::Instant::now() + real_limit(Duration::from_secs(10));
    loop {
        s.time.settle().await;
        if s.fake().record().prompts().len() >= count {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "actual prompt delivery cap: 10 s"
        );
        let hub = s.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
    }
}
pub fn cwd(s: &Scenario) {
    let observed = observations(s);
    assert_eq!(observed.len(), 2, "both actual session creations observed");
    let mut paths = Vec::new();
    for observation in observed {
        let path = PathBuf::from(observation["cwd"].as_str().unwrap());
        assert!(
            path.starts_with(s.root.path.join("temp/acp2api")),
            "session cwd must use the account-owned temporary hierarchy"
        );
        assert_eq!(
            observation["entries"],
            json!([]),
            "actual cwd initially empty"
        );
        assert!(path.is_dir());
        paths.push(path);
    }
    assert_ne!(paths[0], paths[1], "each session gets a distinct cwd");
    assert_ne!(
        s.fake().record().prompts()[0].session,
        s.fake().record().prompts()[1].session
    );
}

pub fn unpermitted_cancelled(s: &Scenario) {
    let prompts = s.fake().record().prompts();
    let cancels = s.fake().record().cancels();
    assert_eq!(prompts.len(), 1, "the corresponding actual prompt exists");
    assert_eq!(cancels.len(), 1, "one actual cancellation delivered");
    assert_eq!(cancels[0]["params"]["sessionId"], prompts[0].session);
    assert!(cancels[0].get("id").is_none(), "cancel is a notification");
    assert!(
        tool_check::permission_requests(s).is_empty(),
        "no permission request occurred"
    );
    assert!(
        !s.fake().record().offered_tools().is_empty(),
        "actual MCP tools/list was observed"
    );
    assert!(
        s.hub
            .entries()
            .iter()
            .all(|entry| entry.event["request"]["method"] != "tools/call"),
        "no MCP execution or held call"
    );
}
