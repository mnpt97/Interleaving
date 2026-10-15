#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{HttpConfig, HttpMethod, RequestBody};
use interl_http::{HttpErrorKind, HttpSession, ResolvedBody, MAX_SESSION_HEADER_BYTES};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

fn config(path: &str) -> HttpConfig {
    HttpConfig {
        method: HttpMethod::Get,
        path: path.into(),
        headers: Default::default(),
        timeout_ms: None,
        body: None,
    }
}
fn headers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect()
}
fn session(limit: usize) -> HttpSession {
    let mut doc = support::scenario("http://127.0.0.1:1", "/health");
    doc.client_profiles[0].headers =
        headers(&[("Authorization", "profile-token"), ("X-Default", "kept")]);
    HttpSession::new(&doc.client_profiles[0], 65536, limit).unwrap()
}

#[test]
fn precedence_removal_and_replacement_are_case_insensitive() {
    let mut session = session(65536);
    let request = config("/health");
    session
        .update_headers(&headers(&[("AUTHORIZATION", "session-token")]), &[])
        .unwrap();
    let prepared = session.prepare(&request, &Default::default()).unwrap();
    assert_eq!(prepared.headers()["authorization"], "session-token");
    assert!(prepared.headers()["authorization"].is_sensitive());
    assert_eq!(prepared.headers()["x-default"], "kept");
    session
        .update_headers(&headers(&[("authorization", "replacement")]), &[])
        .unwrap();
    assert_eq!(
        session
            .prepare(&request, &Default::default())
            .unwrap()
            .headers()["authorization"],
        "replacement"
    );
    session
        .update_headers(&Default::default(), &["aUtHoRiZaTiOn".into()])
        .unwrap();
    assert!(!session
        .prepare(&request, &Default::default())
        .unwrap()
        .headers()
        .contains_key("authorization"));
    // An already-prepared request is an immutable snapshot of its headers.
    assert_eq!(prepared.headers()["authorization"], "session-token");
    let mut explicit = request.clone();
    explicit
        .headers
        .insert("Authorization".into(), "request-token".into());
    assert_eq!(
        session
            .prepare(&explicit, &Default::default())
            .unwrap()
            .headers()["authorization"],
        "request-token"
    );
    // Request override does not clear a removal tombstone.
    assert!(!session
        .prepare(&request, &Default::default())
        .unwrap()
        .headers()
        .contains_key("authorization"));
    session
        .update_headers(&headers(&[("authorization", "restored")]), &[])
        .unwrap();
    assert_eq!(
        session
            .prepare(&request, &Default::default())
            .unwrap()
            .headers()["authorization"],
        "restored"
    );
}

#[test]
fn invalid_updates_leave_values_and_tombstones_unchanged() {
    let mut session = session(65536);
    session
        .update_headers(
            &headers(&[("x-current", "original")]),
            &["authorization".into()],
        )
        .unwrap();
    let before = session
        .prepare(&config("/"), &Default::default())
        .unwrap()
        .headers()
        .clone();
    let invalid = vec![
        (
            headers(&[
                ("x-current", "changed"),
                ("z-invalid", "private-marker\r\nInjected: yes"),
            ]),
            vec![],
        ),
        (
            headers(&[("x-current", "changed")]),
            vec!["x-default".into(), "bad name".into()],
        ),
        (
            headers(&[("X-Current", "changed"), ("x-current", "different")]),
            vec![],
        ),
        (
            headers(&[("authorization", "private-marker")]),
            vec!["AUTHORIZATION".into()],
        ),
        (
            Default::default(),
            vec!["X-Default".into(), "x-default".into()],
        ),
        (headers(&[("bad name", "private-marker")]), vec![]),
        (headers(&[("x-current", "private-marker\0")]), vec![]),
        (headers(&[("x-current", "private-marker\u{7f}")]), vec![]),
        (
            headers(&[("x-current", &"x".repeat(MAX_SESSION_HEADER_BYTES + 1))]),
            vec![],
        ),
        (headers(&[(&"x".repeat(129), "private-marker")]), vec![]),
        (Default::default(), vec![]),
    ];
    for (set, remove) in invalid {
        let error = session.update_headers(&set, &remove).err().unwrap();
        assert_eq!(error.kind, HttpErrorKind::Configuration);
        assert!(!error.to_string().contains("private-marker"));
        assert_eq!(
            session
                .prepare(&config("/"), &Default::default())
                .unwrap()
                .headers(),
            &before
        );
    }
}

#[test]
fn rendered_header_limits_apply_to_bytes_and_allow_tabs() {
    let mut session = session(65536);
    let name = "x".repeat(128);
    let value = "é".repeat(MAX_SESSION_HEADER_BYTES / 2);
    session
        .update_headers(
            &headers(&[(&name, &value), ("x-tab", "Bearer\tvalue")]),
            &[],
        )
        .unwrap();
    let prepared = session.prepare(&config("/"), &Default::default()).unwrap();
    assert_eq!(
        prepared.headers()[name.as_str()].as_bytes(),
        value.as_bytes()
    );
    assert!(session
        .update_headers(&headers(&[(&name, &(value + "é"))]), &[])
        .is_err());
}

#[test]
fn resolved_body_limits_use_serialized_bytes_and_paths_use_encoded_bytes() {
    let session = session(16);
    let request = config("/");
    let exact = json!("a".repeat(14)); // JSON quotes bring this to exactly 16 bytes.
    assert_eq!(
        session
            .prepare_resolved(&request, &Default::default(), ResolvedBody::Json(exact))
            .unwrap()
            .body_bytes(),
        16
    );
    for value in [
        json!("a".repeat(15)),
        json!("\n".repeat(8)),
        json!("é".repeat(8)),
    ] {
        let error = session
            .prepare_resolved(&request, &Default::default(), ResolvedBody::Json(value))
            .err()
            .unwrap();
        assert_eq!(error.kind, HttpErrorKind::Configuration);
        assert!(!error.message.contains("é"));
    }
    assert_eq!(
        session
            .prepare_resolved(
                &request,
                &Default::default(),
                ResolvedBody::Text("é".repeat(8))
            )
            .unwrap()
            .body_bytes(),
        16
    );
    assert!(session
        .prepare_resolved(
            &request,
            &Default::default(),
            ResolvedBody::Text("é".repeat(9))
        )
        .is_err());
    assert!(session
        .prepare(&config("/éééé"), &Default::default())
        .is_err());
    assert!(session.prepare(&config("/éé"), &Default::default()).is_ok());
}

#[tokio::test]
async fn resolved_json_preserves_literals_headers_and_normal_http_behavior() {
    let server = support::TestServer::start().await;
    let mut doc = support::scenario(&server.base_url, "/health");
    doc.client_profiles[0].headers = headers(&[("Content-Type", "profile/type")]);
    let mut session = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    session
        .update_headers(&headers(&[("content-type", "session/type")]), &[])
        .unwrap();
    let mut request = config("/echo?q=${query}");
    request.method = HttpMethod::Post;
    request.body = Some(RequestBody::JsonTemplate {
        fields: Default::default(),
    });
    request.headers = headers(&[("CONTENT-TYPE", "request/type")]);
    let variables = BTreeMap::from([("query".into(), json!("a&b"))]);
    let value = json!({"username":"alice","password":"alice-password","literal":{"source":"identity","field":"password"},"text":"${untouched}"});
    assert!(session.prepare(&request, &variables).is_err());
    let response = session
        .execute(
            session
                .prepare_resolved(&request, &variables, ResolvedBody::Json(value.clone()))
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let echo: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(echo["body"], value);
    assert_eq!(echo["headers"]["content-type"], "request/type");
    assert_eq!(echo["query"], "q=a%26b");
    request.headers.clear();
    assert_eq!(
        session
            .prepare_resolved(&request, &variables, ResolvedBody::Json(json!({})))
            .unwrap()
            .headers()["content-type"],
        "session/type"
    );
    session
        .update_headers(&Default::default(), &["content-type".into()])
        .unwrap();
    assert_eq!(
        session
            .prepare_resolved(&request, &variables, ResolvedBody::Json(json!({})))
            .unwrap()
            .headers()["content-type"],
        "application/json"
    );
}

#[tokio::test]
async fn header_updates_keep_the_authenticated_cookie_jar() {
    let server = support::TestServer::start().await;
    let doc = support::scenario(&server.base_url, "/health");
    let mut session = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    let mut login = config("/auth/login");
    login.method = HttpMethod::Post;
    let response = session
        .execute(
            session
                .prepare_resolved(
                    &login,
                    &Default::default(),
                    ResolvedBody::Json(json!({"username":"alice","password":"alice-password"})),
                )
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    let login: Value = serde_json::from_slice(&response.body).unwrap();
    let token = format!("Bearer {}", login["accessToken"].as_str().unwrap());
    session
        .update_headers(&headers(&[("authorization", &token)]), &[])
        .unwrap();
    for path in ["/auth/me", "/auth/cookie-me"] {
        let response = session
            .execute(
                session.prepare(&config(path), &Default::default()).unwrap(),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["sessionId"], login["sessionId"]);
    }
    session
        .update_headers(&headers(&[("x-extra", "kept")]), &["authorization".into()])
        .unwrap();
    for (path, status) in [("/auth/me", 401), ("/auth/cookie-me", 200)] {
        let response = session
            .execute(
                session.prepare(&config(path), &Default::default()).unwrap(),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, status);
        if status == 200 {
            let body: Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(body["sessionId"], login["sessionId"]);
        }
    }
}

#[test]
fn header_capacity_failures_preserve_session_state_and_never_panic_on_merge() {
    let mut session = session(65536);
    let first: BTreeMap<String, String> = (0..20000)
        .map(|i| (format!("x-first-{i}"), "value".into()))
        .collect();
    let second: BTreeMap<String, String> = (0..20000)
        .map(|i| (format!("x-second-{i}"), "value".into()))
        .collect();
    session.update_headers(&first, &[]).unwrap();
    let error = session
        .update_headers(&second, &["authorization".into()])
        .err()
        .unwrap();
    assert_eq!(error.kind, HttpErrorKind::Configuration);
    let prepared = session.prepare(&config("/"), &Default::default()).unwrap();
    assert_eq!(prepared.headers().len(), 20002);
    assert_eq!(prepared.headers()["authorization"], "profile-token");
    assert!(!prepared.headers().contains_key("x-second-0"));
    let mut request = config("/");
    request.headers = second;
    assert_eq!(
        session
            .prepare(&request, &Default::default())
            .err()
            .unwrap()
            .kind,
        HttpErrorKind::Configuration
    );
}
