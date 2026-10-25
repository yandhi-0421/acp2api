use crate::World;
use cucumber::given;
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
    if world.usage.active {
        world.usage.reply = text;
        return;
    }
    world.scenario.fake().next_prompt(
        acp2api_acceptance::TurnPlan::new()
            .chunk(&text)
            .usage(1000, 1_048_576)
            .end("end_turn"),
    );
}
