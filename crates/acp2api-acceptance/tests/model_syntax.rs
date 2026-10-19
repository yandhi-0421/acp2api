mod selection;
use cucumber::Parser;
use futures_util::StreamExt;
#[tokio::test]
async fn model_literal_template_survives_all_three_examples() {
    let parser = selection::Selected::new(&["--tags".into(), "@issue-43".into()], "not @wip");
    let fs = parser
        .parse(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../features/acp2api"),
            cucumber::cli::Empty {},
        )
        .collect::<Vec<_>>()
        .await;
    let ss: Vec<_> = fs
        .into_iter()
        .flat_map(|f| f.expect("valid owned feature syntax").scenarios)
        .collect();
    assert_eq!(ss.len(), 10);
    let invalid: Vec<_> = ss
        .iter()
        .filter(|s| s.name == "A request with a model that is not a model name gets HTTP 400")
        .collect();
    assert_eq!(invalid.len(), 3);
    for (s, m) in invalid
        .into_iter()
        .zip(["claude-opus-5-5", "gpt-5.5", "gemini-3.8-flash-xhigh"])
    {
        assert_eq!(
            s.steps[1].value,
            format!(
                "acp2api refuses the request with HTTP 400 and the message \"acp2api: the model name {m} is not available. The model names are: <names>.\""
            )
        );
    }
}
