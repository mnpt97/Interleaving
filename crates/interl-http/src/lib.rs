//! One isolated HTTP client per virtual user. No workflow or adapter behavior.
use interl_core::{ClientProfile, HttpConfig, HttpMethod, RequestBody};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
pub use reqwest::{header::HeaderMap, Url};
use reqwest::{Client, Method, Request};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, error::Error, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub const MAX_TIMEOUT_MS: u64 = 300_000;
pub fn encode_component(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HttpErrorKind {
    Configuration,
    Dns,
    Tls,
    Connect,
    Timeout,
    Cancelled,
    Body,
    Transport,
}

#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[error("{message}")]
pub struct HttpError {
    pub kind: HttpErrorKind,
    pub message: String,
}
impl HttpError {
    pub fn configuration(message: &str) -> Self {
        Self {
            kind: HttpErrorKind::Configuration,
            message: message.into(),
        }
    }
    fn from_request(error: reqwest::Error) -> Self {
        let mut source: Option<&(dyn Error + 'static)> = Some(&error);
        let mut kind = if error.is_timeout() {
            HttpErrorKind::Timeout
        } else if error.is_connect() {
            HttpErrorKind::Connect
        } else if error.is_body() || error.is_decode() {
            HttpErrorKind::Body
        } else {
            HttpErrorKind::Transport
        };
        while let Some(cause) = source {
            if cause.is::<DnsFailure>() {
                kind = HttpErrorKind::Dns;
                break;
            }
            if cause.is::<rustls::Error>() {
                kind = HttpErrorKind::Tls;
                break;
            }
            // io::Error::source skips its wrapped error itself; inspect get_ref too
            // so rustls handshake/certificate errors retain their TLS category.
            source = cause
                .downcast_ref::<std::io::Error>()
                .and_then(|error| error.get_ref().map(|inner| inner as &(dyn Error + 'static)))
                .or_else(|| cause.source());
        }
        let message = match kind {
            HttpErrorKind::Dns => "DNS resolution failed",
            HttpErrorKind::Tls => "TLS negotiation or certificate verification failed",
            HttpErrorKind::Connect => "Connection failed",
            HttpErrorKind::Timeout => "Request timed out",
            HttpErrorKind::Body => "Response body could not be read",
            _ => "HTTP transport failed",
        };
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("DNS resolution failed")]
struct DnsFailure;
struct SystemResolver;
impl reqwest::dns::Resolve for SystemResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host, 0))
                .await
                .map_err(|_| Box::new(DnsFailure) as Box<dyn Error + Send + Sync>)?;
            Ok(Box::new(addresses) as reqwest::dns::Addrs)
        })
    }
}

/// Raw request/response data deliberately do not implement Serialize or Debug.
pub struct PreparedRequest {
    request: Request,
    pub body: Option<Value>,
}
impl PreparedRequest {
    pub fn method(&self) -> &str {
        self.request.method().as_str()
    }
    pub fn url(&self) -> &Url {
        self.request.url()
    }
    pub fn headers(&self) -> &HeaderMap {
        self.request.headers()
    }
}
pub struct HttpResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub truncated: bool,
}

pub struct HttpSession {
    client: Client,
    headers: HeaderMap,
    base_url: Url,
    timeout_ms: u64,
    max_response_bytes: usize,
    max_request_bytes: usize,
}
impl HttpSession {
    pub fn new(
        profile: &ClientProfile,
        max_response_bytes: usize,
        max_request_bytes: usize,
    ) -> Result<Self, HttpError> {
        check_timeout(profile.timeout_ms)?;
        if max_response_bytes == 0
            || max_response_bytes > 1_048_576
            || max_request_bytes == 0
            || max_request_bytes > 1_048_576
        {
            return Err(HttpError::configuration(
                "HTTP byte limits must be between 1 and 1048576",
            ));
        }
        let base_url = Url::parse(&profile.base_url)
            .map_err(|_| HttpError::configuration("Invalid base URL"))?;
        if !matches!(base_url.scheme(), "http" | "https")
            || !base_url.username().is_empty()
            || base_url.password().is_some()
        {
            return Err(HttpError::configuration(
                "Base URL must use HTTP(S) without embedded credentials",
            ));
        }
        let headers = parse_headers(&profile.headers)?;
        let client = Client::builder()
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .dns_resolver(Arc::new(SystemResolver))
            .build()
            .map_err(|_| HttpError::configuration("HTTP client could not be initialized"))?;
        Ok(Self {
            client,
            headers,
            base_url,
            timeout_ms: profile.timeout_ms,
            max_response_bytes,
            max_request_bytes,
        })
    }

    pub fn prepare(
        &self,
        config: &HttpConfig,
        variables: &BTreeMap<String, Value>,
    ) -> Result<PreparedRequest, HttpError> {
        let timeout_ms = config.timeout_ms.unwrap_or(self.timeout_ms);
        check_timeout(timeout_ms)?;
        let path = interpolate_path(&config.path, variables)?;
        if path.len() > self.max_request_bytes
            || !path.starts_with('/')
            || path.starts_with("//")
            || path.contains(['\r', '\n', '#'])
        {
            return Err(HttpError::configuration(
                "Request path must be root-relative, bounded, and have no fragment or line breaks",
            ));
        }
        let url = self
            .base_url
            .join(&path)
            .map_err(|_| HttpError::configuration("Invalid request path"))?;
        if url.origin() != self.base_url.origin() {
            return Err(HttpError::configuration(
                "Request path must preserve the client origin",
            ));
        }
        let method = match config.method {
            HttpMethod::Get => Method::GET,
            HttpMethod::Post => Method::POST,
            HttpMethod::Put => Method::PUT,
            HttpMethod::Patch => Method::PATCH,
            HttpMethod::Delete => Method::DELETE,
        };
        let mut headers = self.headers.clone();
        headers.extend(parse_headers(&config.headers)?);
        let has_content_type = headers.contains_key(reqwest::header::CONTENT_TYPE);
        let mut builder = self
            .client
            .request(method, url)
            .timeout(Duration::from_millis(timeout_ms))
            .headers(headers);
        let body = match &config.body {
            Some(RequestBody::Json { value }) => {
                let encoded = serde_json::to_vec(value)
                    .map_err(|_| HttpError::configuration("Invalid JSON request body"))?;
                if encoded.len() > self.max_request_bytes {
                    return Err(HttpError::configuration(
                        "Request body exceeds the byte limit",
                    ));
                }
                if !has_content_type {
                    builder = builder.header(reqwest::header::CONTENT_TYPE, "application/json");
                }
                builder = builder.body(encoded);
                Some(value.clone())
            }
            Some(RequestBody::Text { value }) => {
                if value.len() > self.max_request_bytes {
                    return Err(HttpError::configuration(
                        "Request body exceeds the byte limit",
                    ));
                }
                builder = builder.body(value.clone());
                None
            }
            None => None,
        };
        let request = builder
            .build()
            .map_err(|_| HttpError::configuration("Request could not be constructed"))?;
        Ok(PreparedRequest { request, body })
    }

    pub async fn execute(
        &self,
        prepared: PreparedRequest,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse, HttpError> {
        let fetch = async {
            let mut response = self
                .client
                .execute(prepared.request)
                .await
                .map_err(HttpError::from_request)?;
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let mut body = Vec::new();
            let mut truncated = false;
            while let Some(chunk) = response.chunk().await.map_err(HttpError::from_request)? {
                let available = self.max_response_bytes - body.len();
                body.extend_from_slice(&chunk[..chunk.len().min(available)]);
                if chunk.len() > available {
                    truncated = true;
                    break;
                }
            }
            Ok(HttpResponse {
                status,
                headers,
                body,
                truncated,
            })
        };
        tokio::select! { biased;
            _ = cancel.cancelled() => Err(HttpError { kind: HttpErrorKind::Cancelled, message: "Run cancelled".into() }),
            result = fetch => result,
        }
    }
}

fn check_timeout(ms: u64) -> Result<(), HttpError> {
    if ms == 0 || ms > MAX_TIMEOUT_MS {
        Err(HttpError::configuration(
            "Timeout must be between 1 and 300000 ms",
        ))
    } else {
        Ok(())
    }
}
fn parse_headers(headers: &BTreeMap<String, String>) -> Result<HeaderMap, HttpError> {
    let mut result = HeaderMap::new();
    for (key, value) in headers {
        let name = reqwest::header::HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| HttpError::configuration("Invalid header name"))?;
        let mut value = reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| HttpError::configuration("Invalid header value"))?;
        value.set_sensitive(true);
        result.insert(name, value);
    }
    Ok(result)
}

/// M2 resolves environment scalars only. Substitutions cannot introduce URL delimiters.
pub fn interpolate_path(
    path: &str,
    variables: &BTreeMap<String, Value>,
) -> Result<String, HttpError> {
    let mut output = String::new();
    let mut rest = path;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        let end = rest
            .find('}')
            .ok_or_else(|| HttpError::configuration("Unterminated path variable"))?;
        let key = &rest[..end];
        if key.is_empty()
            || !key.bytes().enumerate().all(|(i, ch)| {
                ch == b'_' || ch.is_ascii_alphabetic() || (i > 0 && ch.is_ascii_digit())
            })
        {
            return Err(HttpError::configuration(
                "Path variables must use ASCII identifiers",
            ));
        }
        let value = match variables.get(key) {
            Some(Value::String(s)) => s.clone(),
            Some(v @ (Value::Bool(_) | Value::Number(_))) => v.to_string(),
            _ => {
                return Err(HttpError::configuration(
                    "Path variable is missing or is not a scalar",
                ))
            }
        };
        if matches!(value.as_str(), "." | "..") {
            return Err(HttpError::configuration(
                "Path substitutions cannot be dot segments",
            ));
        }
        output.push_str(&utf8_percent_encode(&value, NON_ALPHANUMERIC).to_string());
        rest = &rest[end + 1..];
        if output.len() > 1_048_576 {
            return Err(HttpError::configuration(
                "Expanded path exceeds the byte limit",
            ));
        }
    }
    output.push_str(rest);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Unresolvable;
    impl reqwest::dns::Resolve for Unresolvable {
        fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
            Box::pin(async { Err(Box::new(DnsFailure) as Box<dyn Error + Send + Sync>) })
        }
    }
    #[tokio::test]
    async fn dns_failure_has_its_own_category_without_external_dns() {
        let client = Client::builder()
            .no_proxy()
            .dns_resolver(Arc::new(Unresolvable))
            .build()
            .unwrap();
        let error = client.get("http://test.invalid/").send().await.unwrap_err();
        assert_eq!(HttpError::from_request(error).kind, HttpErrorKind::Dns);
    }
}
