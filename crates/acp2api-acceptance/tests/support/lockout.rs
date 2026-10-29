use crate::{World, places_support as queue, quota_support as quota, runtime_support as process};
use acp2api_acceptance::{TurnEndPlan, TurnPlan, TurnStep, client::Answer, endpoint_check};
use serde_json::{Value, json};
use std::path::PathBuf;

pub const SINCE: &str = "2026-10-18T00:00:00Z";
pub const SIGNAL: &str = "the stderr error line 403 \"PERMISSION_DENIED\"";

#[derive(Debug, Default)]
pub struct LockoutState {
    pub calls_before: usize,
    #[cfg(unix)]
    pub unreadable: Option<(PathBuf, std::fs::Permissions)>,
    #[cfg(windows)]
    pub unreadable: Option<std::fs::File>,
}

impl LockoutState {
    pub fn restore(&mut self) {
        #[cfg(unix)]
        if let Some((path, permissions)) = self.unreadable.take() {
            std::fs::set_permissions(path, permissions).unwrap();
        }
        #[cfg(windows)]
        self.unreadable.take();
    }
}

impl Drop for LockoutState {
    fn drop(&mut self) {
        self.restore();
    }
}

pub fn state_path(w: &World) -> PathBuf {
    w.scenario.root.path.join("data/state.json")
}

pub async fn saved(w: &mut World) {
    w.scenario.start().await;
    w.scenario.stop().await;
    std::fs::write(
        state_path(w),
        json!({"format":1,"lockout":{"since":SINCE,"signal":"403 PERMISSION_DENIED","original_error_text":"Synthetic account suspension"},"checked_builds":[]}).to_string(),
    ).unwrap();
}

pub fn plan(description: &str, reply: &str) -> TurnPlan {
    let mut plan = TurnPlan::new();
    if !reply.is_empty() {
        plan.steps.push(TurnStep::Chunk(reply.into()));
    }
    if description == "an exit of the official runtime" {
        plan.end = TurnEndPlan::Exit(17);
    } else if description.starts_with("an error of the ACP call") {
        let quoted: Vec<_> = description.split('"').skip(1).step_by(2).collect();
        plan.end = TurnEndPlan::RpcError {
            code: if description.contains("-32000") {
                -32000
            } else {
                -32603
            },
            message: quoted.join(": "),
        };
    } else {
        plan.steps.push(TurnStep::SessionStderr(
            "Antigravity SDK error processing prompt for session {session}".into(),
        ));
        let quoted: Vec<_> = description.split('"').skip(1).step_by(2).collect();
        let fields = description.strip_prefix("the stderr error line ").unwrap();
        let has_status = fields.starts_with('"')
            || fields.starts_with("with the status ")
            || fields
                .split_once(' ')
                .is_some_and(|(_, rest)| rest.starts_with('"'));
        let status = if has_status { quoted[0] } else { "" };
        let message = quoted
            .get(usize::from(has_status))
            .copied()
            .unwrap_or("Synthetic upstream error");
        if description.contains("and no 403") || description.contains("and no 401") {
            plan.steps.push(TurnStep::Stderr(format!(
                "Message: {message}, Status: {status}, Details: []"
            )));
        } else {
            let code = description
                .strip_prefix("the stderr error line ")
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let message = if description.contains("a 403 in its message") {
                "Synthetic account error 403"
            } else {
                message
            };
            plan.steps.push(TurnStep::ErrorLine {
                code,
                status: status.into(),
                message: message.into(),
            });
        }
    }
    plan
}

pub async fn send_signal(w: &mut World, description: &str) {
    w.request.plan = Some(plan(description, ""));
    quota::send(
        w,
        endpoint_check::request("POST /v1/messages", "lockout signal", false),
    )
    .await;
    assert!(
        !process::calls(w, "session/prompt").is_empty(),
        "signal request reaches the fake ACP agent"
    );
}

pub async fn panel(w: &mut World) -> String {
    w.panel.scenario(&mut w.scenario).await;
    w.panel.login_initial("sign-in-owner").await;
    let answer = w.panel.page("sign-in-owner").await;
    assert_eq!(
        answer.status, 200,
        "owner can observe the state through the panel"
    );
    acp2api_acceptance::panel_check::text(&answer)
}

pub fn refusal(w: &World, answer: &Answer, template: &str) {
    assert_eq!(answer.status, 400, "FR-033 lockout refuses with HTTP 400");
    assert_eq!(
        answer.headers.get("x-should-retry").map(String::as_str),
        Some("false"),
        "FR-033 lockout must not be retried"
    );
    let body = answer.json();
    assert_eq!(body["error"]["type"], "invalid_request_error");
    if body.get("type").is_some() {
        assert_eq!(body["type"], "error");
    }
    let actual = body["error"]["message"].as_str().unwrap();
    let prefix = format!("acp2api: lockout since {SINCE} after ");
    assert!(
        actual.starts_with(&prefix),
        "FR-035 M-12 start time: {actual}"
    );
    let signal = actual
        .strip_prefix(&prefix)
        .unwrap()
        .split_once(';')
        .unwrap()
        .0;
    assert!(!signal.is_empty(), "M-12 identifies the lockout signal");
    let original = if template.contains("<original error text>") {
        let text = actual
            .split_once(" Original error: ")
            .expect("M-12 original error suffix")
            .1;
        assert!(!text.is_empty(), "M-12 original error text is present");
        text
    } else {
        ""
    };
    let expected = template
        .replace("<start time>", SINCE)
        .replace("<signal>", signal)
        .replace(
            "<address of the management panel>",
            &format!("http://127.0.0.1:{}/", w.scenario.root.port),
        )
        .replace("<original error text>", original);
    assert_eq!(actual, expected, "FR-035 M-12 message");
}

pub async fn finish_active(w: &mut World, name: &str, gate: &str) {
    w.scenario.fake().finish_gate(gate, plan(SIGNAL, ""));
    w.scenario.time.settle().await;
    w.scenario.answer = Some(queue::take(w, name).finish(&mut w.scenario.time).await);
}

pub fn state(w: &World) -> Value {
    assert!(
        state_path(w).is_file(),
        "FR-035 the product writes a state file"
    );
    serde_json::from_slice(&std::fs::read(state_path(w)).unwrap()).expect("state file JSON")
}
