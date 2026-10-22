use acp2api_acceptance::{Scenario, Setup, content_check as c};
use serde_json::{Value, json};
const ENDPOINTS: [&str; 3] = [
    "POST /v1/messages",
    "POST /v1/responses",
    "POST /v1/chat/completions",
];
fn setup() -> Scenario {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s
}
#[tokio::test]
async fn request_body_limit_is_32_mb_with_api_413_envelopes() {
    let message = "acp2api: the request body is larger than 32 MB.";
    for endpoint in ENDPOINTS {
        let mut s = setup();
        let mut call = c::request(endpoint, json!([c::text(endpoint, "Hi")]));
        call.body["padding"] = Value::String("a".repeat(2_200_000));
        c::send(&mut s, call).await;
        assert_eq!(s.answer.as_ref().unwrap().status, 200, "body over 2 MB");
        assert_eq!(
            c::blocks(&s),
            vec![json!([{"type":"text","text":"Hi"}])],
            "prompt of a body over 2 MB"
        );
        s.finish(true).await;

        let mut s = setup();
        let mut call = c::request(endpoint, json!([c::text(endpoint, "Hi")]));
        call.body["padding"] = Value::String("a".repeat(32 * 1024 * 1024));
        c::send(&mut s, call).await;
        let a = s.answer.as_ref().unwrap();
        assert_eq!(a.status, 413, "body over 32 MB");
        let expected = if endpoint.ends_with("messages") {
            json!({"type":"error","error":{"type":"request_too_large","message":message}})
        } else {
            json!({"error":{"message":message,"type":"invalid_request_error","param":null,"code":"request_too_large"}})
        };
        assert_eq!(a.json(), expected, "413 envelope");
        assert!(c::blocks(&s).is_empty(), "413 starts no ACP prompt");
        s.finish(true).await;
    }
}
