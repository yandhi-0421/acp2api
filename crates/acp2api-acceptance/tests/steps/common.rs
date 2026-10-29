use crate::World;
use cucumber::{given, then};
#[then(expr = "the message is {string}")]
fn exact_message(w: &mut World, expected: String) {
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().json()["error"]["message"],
        expected,
        "error message of the current response"
    );
}
#[given("acp2api is in the normal state")]
fn normal(world: &mut World) {
    world.scenario.plan(acp2api_acceptance::Setup::normal());
}
#[given(expr = "the official runtime offers the model name {string}")]
fn model(world: &mut World, name: String) {
    world.scenario.fake().model(&name);
}
#[given(expr = "the official runtime replies {string} to the next prompt")]
fn reply(world: &mut World, text: String) {
    world.request.reply = text;
}
