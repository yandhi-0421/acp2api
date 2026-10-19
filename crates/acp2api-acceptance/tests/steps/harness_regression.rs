use super::World;

#[cucumber::given("the harness timeout regression step remains pending")]
async fn pending(_: &mut World) {
    std::future::pending::<()>().await;
}
