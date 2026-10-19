use cucumber::{
    World,
    codegen::{StepConstructor, WorldInventory, inventory},
    step::{Collection, Context},
};
use futures_util::future::LocalBoxFuture;
use std::{fmt::Debug, sync::OnceLock, time::Duration};

fn limit() -> Duration {
    static LIMIT: OnceLock<Duration> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        let config: toml::Value = toml::from_str(include_str!("../runner.toml")).unwrap();
        Duration::from_secs(
            config["step_timeout_seconds"]
                .as_integer()
                .unwrap()
                .try_into()
                .unwrap(),
        )
    })
}
fn execute<W: World + WorldInventory + Debug>(
    world: &mut W,
    context: Context,
) -> LocalBoxFuture<'_, ()> {
    Box::pin(async move {
        let collection = W::collection();
        let (step, _, _, original_context) = collection
            .find(&context.step)
            .expect("unambiguous step")
            .expect("registered step");
        let step = *step;
        tokio::time::timeout(limit(), step(world, original_context))
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "acceptance step exceeded {} s: {}",
                    limit().as_secs(),
                    context.step.value
                )
            });
    })
}
pub fn collection<W: World + WorldInventory + Debug>() -> Collection<W> {
    let mut steps = Collection::new();
    for given in inventory::iter::<W::Given> {
        let (loc, regex, _) = given.inner();
        steps = steps.given(Some(loc), regex(), execute::<W>);
    }
    for when in inventory::iter::<W::When> {
        let (loc, regex, _) = when.inner();
        steps = steps.when(Some(loc), regex(), execute::<W>);
    }
    for then in inventory::iter::<W::Then> {
        let (loc, regex, _) = then.inner();
        steps = steps.then(Some(loc), regex(), execute::<W>);
    }
    steps
}
