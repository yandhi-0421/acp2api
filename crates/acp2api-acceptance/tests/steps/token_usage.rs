use crate::World;
use acp2api_acceptance::client::ApiCall;
use cucumber::{then, when};
#[when(expr = "an API client sends a valid request to {string}")]
async fn send(world: &mut World, endpoint: String) {
    if endpoint == "POST /v1/messages/count_tokens" {
        let call = world
            .scenario
            .client()
            .await
            .send(ApiCall::one_user_message(&endpoint, "Hello"));
        world.scenario.answer = Some(call.finish(&mut world.scenario.time).await);
        return;
    }
    world.request.queue_next(&world.scenario);
    let call = world
        .scenario
        .client()
        .await
        .send(world.request.request(&endpoint, false));
    let answer = call.finish(&mut world.scenario.time).await;
    assert_eq!(answer.status, 200, "request must reach the product");
    world.scenario.answer = Some(answer);
}
#[then(expr = "the usage of the response has {string} set to {int}")]
fn usage(world: &mut World, field: String, expected: u64) {
    assert_eq!(
        world.scenario.answer.as_ref().expect("answer").json()["usage"][field].as_u64(),
        Some(expected),
        "FR-025 output token count"
    );
}
