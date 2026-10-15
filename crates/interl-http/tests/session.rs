#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{HttpConfig, HttpMethod, RequestBody, WorkflowNode};
use interl_http::{HttpErrorKind, HttpSession};
use serde_json::{json, Value};
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
#[tokio::test]
async fn post_headers_interpolation_and_http_errors() {
    let server = support::TestServer::start().await;
    let mut document = support::scenario(&server.base_url, "/health");
    document.client_profiles[0]
        .headers
        .insert("X-Profile".into(), "profile".into());
    let session = HttpSession::new(&document.client_profiles[0], 65536, 65536).unwrap();
    let mut request = config("/echo?q=${query}");
    request.method = HttpMethod::Post;
    request
        .headers
        .insert("x-profile".into(), "override".into());
    request
        .headers
        .insert("Content-Type".into(), "application/vnd.interl+json".into());
    request.body = Some(RequestBody::Json {
        value: json!({"hello":"world"}),
    });
    document
        .environment
        .variables
        .insert("query".into(), json!("a&b/c?d"));
    let response = session
        .execute(
            session
                .prepare(&request, &document.environment.variables)
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(body["method"], "POST");
    assert_eq!(body["body"]["hello"], "world");
    assert_eq!(body["headers"]["x-profile"], "override");
    assert_eq!(
        body["headers"]["content-type"],
        "application/vnd.interl+json"
    );
    assert_eq!(body["query"], "q=a%26b%2Fc%3Fd");
    for (path, status) in [("/not-found", 404), ("/redirect", 307)] {
        let response = session
            .execute(
                session.prepare(&config(path), &Default::default()).unwrap(),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, status);
    }
    assert_eq!(server.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}
#[tokio::test]
async fn timeout_cancellation_response_limit_and_cookie_isolation() {
    let server = support::TestServer::start().await;
    let doc = support::scenario(&server.base_url, "/health");
    let session = HttpSession::new(&doc.client_profiles[0], 128, 65536).unwrap();
    let mut request = config("/slow");
    request.timeout_ms = Some(20);
    let error = session
        .execute(
            session.prepare(&request, &Default::default()).unwrap(),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, HttpErrorKind::Timeout);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        session
            .execute(
                session
                    .prepare(&config("/health"), &Default::default())
                    .unwrap(),
                &cancel
            )
            .await
            .err()
            .unwrap()
            .kind,
        HttpErrorKind::Cancelled
    );
    let response = session
        .execute(
            session
                .prepare(&config("/large"), &Default::default())
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(response.body.len(), 128);
    assert!(response.truncated);
    let first = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    let second = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    first
        .execute(
            first
                .prepare(&config("/cookie"), &Default::default())
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    for (client, expected) in [(&first, true), (&second, false)] {
        let r = client
            .execute(
                client
                    .prepare(&config("/echo"), &Default::default())
                    .unwrap(),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["headers"].get("cookie").is_some(), expected);
    }
}
#[test]
fn rejects_bad_templates_and_limits_without_network() {
    let document = support::scenario("http://127.0.0.1:1", "/health");
    let session = HttpSession::new(&document.client_profiles[0], 65536, 65536).unwrap();
    for path in [
        "/a/${missing}",
        "/a/${broken",
        "//elsewhere/",
        "/a/${not-valid}",
        "/a#fragment",
    ] {
        assert!(session.prepare(&config(path), &Default::default()).is_err());
    }
    let WorkflowNode::Http { config, .. } = &document.workflows[0].nodes[1] else {
        panic!()
    };
    let mut bad: HttpConfig =
        serde_json::from_value(serde_json::to_value(config).unwrap()).unwrap();
    bad.timeout_ms = Some(u64::MAX);
    assert!(session.prepare(&bad, &Default::default()).is_err());
    bad.timeout_ms = None;
    bad.body = Some(RequestBody::JsonTemplate {
        fields: Default::default(),
    });
    let error = session.prepare(&bad, &Default::default()).err().unwrap();
    assert_eq!(error.kind, HttpErrorKind::Configuration);
    assert!(error.message.contains("must be resolved by the engine"));
}
#[tokio::test]
async fn connection_failure_has_static_safe_diagnostic() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let document = support::scenario(&format!("http://{address}"), "/private?token=never-print");
    let session = HttpSession::new(&document.client_profiles[0], 65536, 65536).unwrap();
    let error = session
        .execute(
            session
                .prepare(&config("/private?token=never-print"), &Default::default())
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, HttpErrorKind::Connect);
    assert!(!error.to_string().contains("never-print"));
}

#[tokio::test]
async fn invalid_tls_peer_is_a_tls_error() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
        // Keep the peer alive until the client rejects its invalid TLS record.
        let _ = tokio::io::copy(&mut stream, &mut tokio::io::sink()).await;
    });
    let doc = support::scenario(&format!("https://{address}"), "/health");
    let session = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    let error = session
        .execute(
            session
                .prepare(&config("/health"), &Default::default())
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, HttpErrorKind::Tls);
    peer.await.unwrap();
}

#[tokio::test]
async fn incomplete_response_body_is_a_body_error() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 4096];
        assert!(stream.read(&mut request).await.unwrap() > 0);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nshort")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
    });
    let doc = support::scenario(&format!("http://{address}"), "/health");
    let session = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    let error = session
        .execute(
            session
                .prepare(&config("/health"), &Default::default())
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, HttpErrorKind::Body);
    peer.await.unwrap();
}
