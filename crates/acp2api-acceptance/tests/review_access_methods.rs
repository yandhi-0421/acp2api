use acp2api_acceptance::{Scenario, Setup, client::ApiCall, model_check};

fn call(method: &str, path: &str) -> ApiCall {
    ApiCall {
        method: method.into(),
        path: path.into(),
        ..model_check::list()
    }
}

#[tokio::test]
async fn the_access_guard_keeps_the_http_method_handling_of_the_router() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s.start().await;
    let head = s
        .client()
        .await
        .send(call("HEAD", "/v1/models"))
        .finish(&mut s.time)
        .await;
    assert_eq!(head.status, 200, "HEAD /v1/models is served as GET");
    for (method, path, allow) in [
        ("POST", "/v1/models", "GET,HEAD"),
        ("GET", "/v1/messages", "POST"),
        ("GET", "/v1/chat/completions", "POST"),
    ] {
        let a = s
            .client()
            .await
            .send(call(method, path))
            .finish(&mut s.time)
            .await;
        assert_eq!(a.status, 405, "{method} {path}");
        assert_eq!(
            a.headers.get("allow").map(String::as_str),
            Some(allow),
            "{method} {path}: the 405 response names the methods of the route"
        );
    }
    s.finish(true).await;
}
