// @critical: Health endpoint test
use super::test_utils::TestServer;
use axum::body::to_bytes;
use axum::http::StatusCode;
use serde_json::Value;

#[tokio::test]
async fn test_health_endpoint() {
    let server = TestServer::new();

    let req = server.make_request("GET", "/health", None, None);
    let response = server.call(req).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body();
    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(json.get("version").is_some());
}
