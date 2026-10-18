use crate::World;
use acp2api_acceptance::client::ApiCall;
use cucumber::{then, when};
#[when(expr = "an API client sends a request to {string} with one unnamed user message {string}")]
async fn send(world: &mut World, endpoint: String, text: String) {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let call = world
            .scenario
            .client()
            .await
            .send(ApiCall::one_user_message(&endpoint, &text));
        let answer = call.finish(&mut world.scenario.time).await;
        assert_eq!(answer.status, 200, "request must reach the product");
        world.scenario.answer = Some(answer);
    })
    .await
    .expect("step cap: 20 s");
}
#[then(expr = "the prompt that the official runtime gets is {string}")]
async fn prompt(world: &mut World, expected: String) {
    world.scenario.time.settle().await;
    assert_eq!(
        world.scenario.fake().prompts().last().expect("a prompt"),
        &expected,
        "FR-020 prompt text"
    );
}
