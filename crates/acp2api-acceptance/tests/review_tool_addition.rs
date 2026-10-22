use acp2api_acceptance::{Scenario, Setup, content_check, server_tool_check as c};

// FR-024: only a tool_reference to a declared tool is ignored. A full tool
// definition gets M-03 also when its name is a declared tool.
#[tokio::test]
async fn a_full_definition_of_a_declared_tool_gets_m03() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    c::send(&mut s, c::with_addition("read_file", false, "read_file")).await;
    c::envelope(
        &s,
        c::MESSAGES,
        "acp2api: the content type tool_addition is not supported.",
        "unsupported_content_type",
    );
    assert!(content_check::blocks(&s).is_empty(), "no ACP prompt");
    s.finish(true).await;
}
