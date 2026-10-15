#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{HttpConfig, HttpMethod, RequestBody};
use interl_http::{HttpErrorKind, HttpSession};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .no_proxy()
        .build()
        .unwrap()
}
fn credentials() -> Value {
    json!({"username":"alice","password":"alice-password"})
}
fn login_config(path: &str) -> HttpConfig {
    HttpConfig {
        method: HttpMethod::Post,
        path: path.into(),
        headers: Default::default(),
        timeout_ms: None,
        body: Some(RequestBody::Json {
            value: credentials(),
        }),
    }
}

#[tokio::test]
async fn valid_login_supports_bearer_and_cookie_access_with_fresh_sessions() {
    let server = support::TestServer::start().await;
    let client = client();
    let first = client
        .post(format!("{}/auth/login", server.base_url))
        .json(&credentials())
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let set_cookie = first.headers()["set-cookie"].to_str().unwrap().to_owned();
    assert!(
        set_cookie.contains("HttpOnly")
            && set_cookie.contains("SameSite=Lax")
            && set_cookie.contains("Path=/")
    );
    let first: Value = first.json().await.unwrap();
    let token = first["accessToken"].as_str().unwrap();
    let bearer: Value = client
        .get(format!("{}/auth/me", server.base_url))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cookie: Value = client
        .get(format!("{}/auth/cookie-me", server.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(bearer, cookie);
    assert_eq!(bearer["user"], "alice");
    assert_eq!(bearer["sessionId"], first["sessionId"]);
    assert!(bearer.get("accessToken").is_none());
    // Cookie authentication does not substitute for an explicit Bearer header.
    assert_eq!(
        client
            .get(format!("{}/auth/me", server.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let second = client
        .post(format!("{}/auth/login", server.base_url))
        .json(&credentials())
        .send()
        .await
        .unwrap();
    assert_ne!(second.headers()["set-cookie"].to_str().unwrap(), set_cookie);
    let second: Value = second.json().await.unwrap();
    assert_ne!(first["accessToken"], second["accessToken"]);
    assert_ne!(first["sessionId"], second["sessionId"]);
    let cookie: Value = client
        .get(format!("{}/auth/cookie-me", server.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cookie["sessionId"], second["sessionId"]);
    let old: Value = client
        .get(format!("{}/auth/me", server.base_url))
        .header("authorization", format!("bearer {token}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(old["sessionId"], first["sessionId"]);
}

#[tokio::test]
async fn fixed_fake_credential_pairs_map_to_the_selected_identity() {
    let server = support::TestServer::start().await;
    let client = client();
    for (user, password) in [("alice", "alice-password"), ("bob", "bob-password")] {
        let body: Value = client
            .post(format!("{}/auth/login", server.base_url))
            .json(&json!({"username":user,"password":password}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let me: Value = client
            .get(format!("{}/auth/me", server.base_url))
            .bearer_auth(body["accessToken"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(me["user"], user);
        assert_eq!(me["sessionId"], body["sessionId"]);
    }
}

#[tokio::test]
async fn invalid_credentials_shapes_and_unknown_auth_are_rejected_without_echoes() {
    let server = support::TestServer::start().await;
    let client = client();
    for (body, status) in [
        (
            json!({"username":"alice","password":"wrong-private-marker"}),
            401,
        ),
        (
            json!({"username":"missing","password":"wrong-private-marker"}),
            401,
        ),
        (json!({"username":"alice"}), 400),
        (json!({"username":"alice","password":123}), 400),
        (
            json!({"username":"alice","password":"alice-password","unexpected":true}),
            400,
        ),
    ] {
        let response = client
            .post(format!("{}/auth/login", server.base_url))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert!(!response.headers().contains_key("set-cookie"));
        assert!(!response
            .text()
            .await
            .unwrap()
            .contains("wrong-private-marker"));
    }
    assert_eq!(
        client
            .post(format!("{}/auth/login", server.base_url))
            .body("raw")
            .send()
            .await
            .unwrap()
            .status(),
        415
    );
    assert_eq!(
        client
            .post(format!("{}/auth/login", server.base_url))
            .header("content-type", "application/json")
            .body("{")
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        client
            .post(format!("{}/auth/login", server.base_url))
            .header("content-type", "application/json")
            .body("x".repeat(4097))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
    for path in ["/auth/me", "/auth/cookie-me"] {
        assert_eq!(
            client
                .get(format!("{}{path}", server.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
    assert_eq!(
        client
            .get(format!("{}/auth/me", server.base_url))
            .bearer_auth("unknown-private-marker")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .get(format!("{}/auth/me", server.base_url))
            .header("authorization", "Basic unknown-private-marker")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client
        .get(format!("{}/auth/cookie-me", server.base_url))
        .header("cookie", "interl_session=unknown-private-marker")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert!(!response
        .text()
        .await
        .unwrap()
        .contains("unknown-private-marker"));
}

#[tokio::test]
async fn ambiguous_auth_headers_and_duplicate_session_cookies_are_rejected() {
    let server = support::TestServer::start().await;
    let client = client();
    let response = client
        .post(format!("{}/auth/login", server.base_url))
        .json(&credentials())
        .send()
        .await
        .unwrap();
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let body: Value = response.json().await.unwrap();
    let token = body["accessToken"].as_str().unwrap();
    assert_eq!(
        client
            .get(format!("{}/auth/me", server.base_url))
            .header("authorization", format!("Bearer {token}"))
            .header("authorization", "Bearer unknown")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .get(format!("{}/auth/cookie-me", server.base_url))
            .header("cookie", format!("{cookie}; {cookie}"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
}

#[tokio::test]
async fn token_response_variants_are_controlled_and_cookie_auth_is_independent() {
    let server = support::TestServer::start().await;
    let client = client();
    let body: Value = client
        .post(format!("{}/auth/login-missing-token", server.base_url))
        .json(&credentials())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body.get("accessToken").is_none());
    let cookie: Value = client
        .get(format!("{}/auth/cookie-me", server.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cookie["sessionId"], body["sessionId"]);
    let body: Value = client
        .post(format!("{}/auth/login-unfamiliar", server.base_url))
        .json(&credentials())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body.get("accessToken").is_none());
    let me = client
        .get(format!("{}/auth/me", server.base_url))
        .bearer_auth(body["opaqueArtifact"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
    let response = client
        .post(format!("{}/auth/login-malformed", server.base_url))
        .json(&credentials())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert!(serde_json::from_slice::<Value>(&response.bytes().await.unwrap()).is_err());
    let response = client
        .post(format!("{}/auth/login-text", server.base_url))
        .json(&credentials())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "text/plain");
    assert!(response.text().await.unwrap().starts_with("fake-token-"));
}

#[tokio::test]
async fn oversized_login_response_exercises_transport_truncation() {
    let server = support::TestServer::start().await;
    let doc = support::scenario(&server.base_url, "/health");
    let session = HttpSession::new(&doc.client_profiles[0], 128, 65536).unwrap();
    let response = session
        .execute(
            session
                .prepare(&login_config("/auth/login-large"), &Default::default())
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body.len(), 128);
    assert!(response.truncated);
    assert!(serde_json::from_slice::<Value>(&response.body).is_err());
}

#[tokio::test]
async fn delayed_login_supports_readiness_based_cancellation_and_timeout() {
    let server = support::TestServer::start().await;
    let doc = support::scenario(&server.base_url, "/health");
    let session = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    let request = session
        .prepare(&login_config("/auth/login-slow"), &Default::default())
        .unwrap();
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task = tokio::spawn(async move { session.execute(request, &task_cancel).await });
    tokio::time::timeout(Duration::from_secs(2), server.auth_entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let error = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .err()
        .unwrap();
    assert_eq!(error.kind, HttpErrorKind::Cancelled);
    let response = client()
        .post(format!("{}/auth/login-slow", server.base_url))
        .json(&credentials())
        .timeout(Duration::from_millis(20))
        .send()
        .await;
    assert!(response.unwrap_err().is_timeout());
}

#[tokio::test]
async fn fixture_session_storage_has_a_fixed_capacity() {
    let auth = support::auth::AuthState::with_capacity(Duration::from_millis(300), 1);
    let server = support::TestServer::start_with_auth(auth).await;
    let client = client();
    assert_eq!(
        client
            .post(format!("{}/auth/login", server.base_url))
            .json(&credentials())
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let response = client
        .post(format!("{}/auth/login", server.base_url))
        .json(&credentials())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert!(!response.headers().contains_key("set-cookie"));
    assert_eq!(
        client
            .get(format!("{}/auth/cookie-me", server.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}
