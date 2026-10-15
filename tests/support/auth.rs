//! Loopback-only fake authentication, shared by tests and the manual example.
//! No production crate imports this module. Do not add credential/token logs.
use axum::body::Bytes;
use axum::extract::DefaultBodyLimit;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::Notify;

pub const COOKIE_NAME: &str = "interl_session";
const MAX_SESSIONS: usize = 1024;

#[derive(Clone)]
pub struct AuthState {
    sessions: Arc<Mutex<Sessions>>,
    pub entered: Arc<Notify>,
    delay: Duration,
    capacity: usize,
}
#[derive(Default)]
struct Sessions {
    next: u64,
    tokens: BTreeMap<String, Session>,
    cookies: BTreeMap<String, String>,
}
#[derive(Clone)]
struct Session {
    user: &'static str,
    id: String,
}
impl AuthState {
    pub fn new(delay: Duration) -> Self {
        Self::with_capacity(delay, MAX_SESSIONS)
    }
    pub fn with_capacity(delay: Duration, capacity: usize) -> Self {
        Self {
            sessions: Arc::default(),
            entered: Arc::default(),
            delay,
            capacity: capacity.min(MAX_SESSIONS),
        }
    }
}
#[derive(Clone, Copy)]
enum Mode {
    Normal,
    MissingToken,
    Malformed,
    Text,
    Unfamiliar,
    Slow,
    Large,
}

pub fn router(state: AuthState) -> axum::Router {
    let mut router = axum::Router::new();
    for (path, mode) in [
        ("/auth/login", Mode::Normal),
        ("/auth/login-missing-token", Mode::MissingToken),
        ("/auth/login-malformed", Mode::Malformed),
        ("/auth/login-text", Mode::Text),
        ("/auth/login-unfamiliar", Mode::Unfamiliar),
        ("/auth/login-slow", Mode::Slow),
        ("/auth/login-large", Mode::Large),
    ] {
        router = router.route(
            path,
            post(
                move |State(state): State<AuthState>, headers: HeaderMap, body: Bytes| async move {
                    login(state, mode, headers, body).await
                },
            ),
        );
    }
    router
        .route("/auth/me", get(bearer_me))
        .route("/auth/cookie-me", get(cookie_me))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(state)
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, axum::Json(serde_json::json!({"error":message}))).into_response()
}
async fn login(state: AuthState, mode: Mode, headers: HeaderMap, body: Bytes) -> Response {
    let json = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
    if !json {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "application/json required",
        );
    }
    let Ok(Value::Object(fields)) = serde_json::from_slice::<Value>(&body) else {
        return error(StatusCode::BAD_REQUEST, "invalid login shape");
    };
    let (Some(username), Some(password)) = (
        fields.get("username").and_then(Value::as_str),
        fields.get("password").and_then(Value::as_str),
    ) else {
        return error(
            StatusCode::BAD_REQUEST,
            "username and password strings required",
        );
    };
    if fields.len() != 2 {
        return error(StatusCode::BAD_REQUEST, "unexpected login fields");
    }
    let user = match (username, password) {
        ("alice", "alice-password") => "alice",
        ("bob", "bob-password") => "bob",
        _ => return error(StatusCode::UNAUTHORIZED, "invalid credentials"),
    };
    if matches!(mode, Mode::Slow) {
        state.entered.notify_one();
        tokio::time::sleep(state.delay).await;
    }
    let (token, cookie, session) = {
        let mut sessions = state.sessions.lock().await;
        if sessions.tokens.len() >= state.capacity {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "fixture session capacity reached; restart server",
            );
        }
        let Some(next) = sessions.next.checked_add(1) else {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "fixture session capacity reached; restart server",
            );
        };
        sessions.next = next;
        // Deliberately predictable fake values. Every accepted login in this
        // server instance gets new values, even with the same credentials.
        let token = format!("fake-token-{next:08}");
        let cookie = format!("fake-cookie-{next:08}");
        let session = Session {
            user,
            id: format!("fake-session-{next:08}"),
        };
        sessions.tokens.insert(token.clone(), session.clone());
        sessions.cookies.insert(cookie.clone(), token.clone());
        (token, cookie, session)
    };
    let body = match mode {
        Mode::Malformed => (
            [("content-type", "application/json")],
            format!("{{\"accessToken\":\"{token}"),
        )
            .into_response(),
        Mode::Text => ([("content-type", "text/plain")], token).into_response(),
        _ => {
            let mut body = serde_json::json!({"user":session.user,"sessionId":session.id});
            match mode {
                Mode::MissingToken => {}
                Mode::Unfamiliar => body["opaqueArtifact"] = Value::String(token),
                _ => body["accessToken"] = Value::String(token),
            }
            if matches!(mode, Mode::Large) {
                body["padding"] = Value::String("x".repeat(131072));
            }
            axum::Json(body).into_response()
        }
    };
    let mut response = body;
    let cookie = format!("{COOKIE_NAME}={cookie}; Path=/; HttpOnly; SameSite=Lax");
    if let Ok(cookie) = cookie.parse() {
        response.headers_mut().insert("set-cookie", cookie);
    }
    response
}

async fn bearer_me(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let mut values = headers.get_all("authorization").iter();
    let token = values
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, token)| {
            scheme.eq_ignore_ascii_case("Bearer")
                && !token.is_empty()
                && !token.chars().any(char::is_whitespace)
        })
        .map(|(_, token)| token);
    if values.next().is_some() {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let session = match token {
        Some(token) => state.sessions.lock().await.tokens.get(token).cloned(),
        None => None,
    };
    session_response(session)
}
async fn cookie_me(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let mut cookie = None;
    for value in headers.get_all("cookie").iter() {
        let Ok(value) = value.to_str() else {
            return error(StatusCode::UNAUTHORIZED, "unauthorized");
        };
        for pair in value.split(';') {
            match pair.trim().split_once('=') {
                Some((name, value)) if name == COOKIE_NAME => {
                    if cookie.is_some() {
                        return error(StatusCode::UNAUTHORIZED, "unauthorized");
                    }
                    cookie = Some(value);
                }
                _ => {}
            }
        }
    }
    let sessions = state.sessions.lock().await;
    let session = cookie
        .and_then(|cookie| sessions.cookies.get(cookie))
        .and_then(|token| sessions.tokens.get(token))
        .cloned();
    session_response(session)
}
fn session_response(session: Option<Session>) -> Response {
    match session {
        Some(session) => {
            axum::Json(serde_json::json!({"user":session.user,"sessionId":session.id}))
                .into_response()
        }
        None => error(StatusCode::UNAUTHORIZED, "unauthorized"),
    }
}
