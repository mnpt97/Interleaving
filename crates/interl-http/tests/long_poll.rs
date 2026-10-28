#[path = "../../../tests/support/mod.rs"]
mod support;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

#[tokio::test]
async fn long_poll_waits_for_the_requested_duration_and_accepts_zero() {
    let server = support::TestServer::start().await;
    let client = client();
    for timeout_ms in [0, 40] {
        let started = Instant::now();
        let response = client
            .post(format!("{}/long-poll", server.base_url))
            .json(&json!({"timeoutMs":timeout_ms}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(started.elapsed() >= Duration::from_millis(timeout_ms));
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"status":"complete","timeoutMs":timeout_ms})
        );
    }
}

#[tokio::test]
async fn long_poll_rejects_invalid_bodies_without_echoing_them() {
    let server = support::TestServer::start().await;
    let client = client();
    for body in [
        "{}",
        r#"{"timeoutMs":-1}"#,
        r#"{"timeoutMs":1.5}"#,
        r#"{"timeoutMs":"private-marker"}"#,
        r#"{"timeoutMs":true}"#,
        r#"{"timeoutMs":null}"#,
        r#"{"timeoutMs":60001}"#,
        r#"{"timeoutMs":18446744073709551616}"#,
        r#"{"timeoutMs":0,"extra":"private-marker"}"#,
        r#"{"timeoutMs":0,"timeoutMs":1}"#,
        "not-json",
    ] {
        let response = client
            .post(format!("{}/long-poll", server.base_url))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "invalid case was accepted");
        assert!(!response.text().await.unwrap().contains("private-marker"));
    }
}

#[tokio::test]
async fn long_poll_requires_post_json_and_bounds_body_size() {
    let server = support::TestServer::start().await;
    let client = client();
    let url = format!("{}/long-poll", server.base_url);
    assert_eq!(client.get(&url).send().await.unwrap().status(), 405);
    assert_eq!(
        client
            .post(&url)
            .body(r#"{"timeoutMs":0}"#)
            .send()
            .await
            .unwrap()
            .status(),
        415
    );
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body("x".repeat(1025))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
}

#[tokio::test]
async fn long_poll_respects_the_clients_shorter_http_deadline() {
    let server = support::TestServer::start().await;
    let result = client()
        .post(format!("{}/long-poll", server.base_url))
        .timeout(Duration::from_millis(20))
        .json(&json!({"timeoutMs":200}))
        .send()
        .await;
    assert!(result.unwrap_err().is_timeout());
}
