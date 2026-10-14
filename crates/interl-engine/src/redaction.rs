use interl_http::{HeaderMap, Url};
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::OnceLock};

const REDACTED: &str = "[REDACTED]";
/// Redaction is applied before data enters an event, snapshot, or report.
pub(crate) struct Redactor {
    secrets: BTreeSet<String>,
    fields: Vec<String>,
    matcher: OnceLock<Option<aho_corasick::AhoCorasick>>,
    secret_bytes: usize,
    exhausted: bool,
}
impl Redactor {
    pub fn new(fields: Vec<String>) -> Self {
        Self {
            secrets: BTreeSet::new(),
            fields: fields
                .into_iter()
                .map(|field| {
                    field
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect()
                })
                .collect(),
            matcher: OnceLock::new(),
            secret_bytes: 0,
            exhausted: false,
        }
    }
    pub fn sensitive(&self, key: &str) -> bool {
        let key: String = key
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        [
            "authorization",
            "cookie",
            "password",
            "passwd",
            "secret",
            "token",
            "apikey",
            "privatekey",
            "credential",
            "session",
        ]
        .iter()
        .any(|part| key.contains(part))
            || self.fields.iter().any(|f| f.eq_ignore_ascii_case(&key))
    }
    fn remember(&mut self, value: &str) {
        if !value.is_empty() && !self.exhausted {
            for secret in [value.to_owned(), interl_http::encode_component(value)] {
                if !self.secrets.contains(&secret) {
                    self.secret_bytes += secret.len();
                    if self.secret_bytes > 1_048_576 || self.secrets.len() >= 4096 {
                        self.exhausted = true;
                        self.secrets.clear();
                        self.matcher.take();
                        return;
                    }
                    self.secrets.insert(secret);
                    self.matcher.take();
                }
            }
            if let Some(token) = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("Basic "))
            {
                self.remember(token);
            }
            for pair in value.split(';') {
                if let Some((_, v)) = pair.split_once('=') {
                    if !v.trim().is_empty() {
                        self.remember(v.trim());
                    }
                }
            }
        }
    }
    pub fn learn(&mut self, value: &Value) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if self.sensitive(key) {
                        self.remember_value(value);
                    }
                    self.learn(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    self.learn(value);
                }
            }
            _ => (),
        }
    }
    pub fn remember_value(&mut self, value: &Value) {
        match value {
            Value::String(s) => self.remember(s),
            Value::Number(_) | Value::Bool(_) => self.remember(&value.to_string()),
            Value::Object(m) => {
                for v in m.values() {
                    self.remember_value(v);
                }
            }
            Value::Array(a) => {
                for v in a {
                    self.remember_value(v);
                }
            }
            _ => (),
        }
    }
    pub fn learn_headers(&mut self, headers: &HeaderMap) {
        for (key, value) in headers {
            if self.sensitive(key.as_str()) {
                if let Ok(value) = value.to_str() {
                    self.remember(value);
                }
            }
        }
    }
    pub fn learn_url(&mut self, url: &Url) {
        for (_, value) in url.query_pairs() {
            self.remember(&value);
        }
        if let Some(query) = url.query() {
            for pair in query.split('&') {
                if let Some((_, value)) = pair.split_once('=') {
                    self.remember(value);
                }
            }
        }
    }
    pub fn text(&self, input: &str) -> String {
        // Replace against the original input once; short secrets cannot expand markers recursively.
        if self.exhausted {
            return REDACTED.into();
        }
        if self.secrets.is_empty() {
            return input.to_owned();
        }
        let matcher = self.matcher.get_or_init(|| {
            aho_corasick::AhoCorasick::builder()
                .match_kind(aho_corasick::MatchKind::LeftmostLongest)
                .build(&self.secrets)
                .ok()
        });
        let Some(matcher) = matcher else {
            return "[Preview omitted]".into();
        };
        let mut output = String::new();
        let mut at = 0;
        for found in matcher.find_iter(input) {
            output.push_str(&input[at..found.start()]);
            output.push_str(REDACTED);
            at = found.end();
        }
        output.push_str(&input[at..]);
        output
    }
    pub fn value(&self, input: &Value) -> Value {
        if self.exhausted {
            return json!("[Preview omitted: redaction budget exceeded]");
        }
        match input {
            Value::Object(m) => Value::Object(
                m.iter()
                    .map(|(k, v)| {
                        (
                            self.text(k),
                            if self.sensitive(k) {
                                Value::String(REDACTED.into())
                            } else {
                                self.value(v)
                            },
                        )
                    })
                    .collect(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(|v| self.value(v)).collect()),
            Value::String(s) => Value::String(self.text(s)),
            Value::Number(_) | Value::Bool(_) if self.secrets.contains(&input.to_string()) => {
                Value::String(REDACTED.into())
            }
            _ => input.clone(),
        }
    }
    pub fn url(&self, input: &str) -> String {
        let Ok(mut url) = Url::parse(input) else {
            return "[URL omitted]".into();
        };
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_fragment(None);
        if url.query().is_some() {
            let keys: Vec<_> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
            url.set_query(None);
            for key in keys {
                url.query_pairs_mut().append_pair(&key, REDACTED);
            }
        }
        self.text(url.as_str())
    }
    pub fn headers(&self, headers: &HeaderMap) -> Value {
        let mut map = serde_json::Map::new();
        let mut bytes = 0;
        for (key, value) in headers.iter().take(64) {
            let value = if self.sensitive(key.as_str()) {
                REDACTED.into()
            } else if matches!(key.as_str(), "location" | "referer" | "refresh" | "link") {
                "[URL header omitted]".into()
            } else {
                self.text(value.to_str().unwrap_or("[binary header omitted]"))
            };
            bytes += key.as_str().len() + value.len();
            if bytes > 8192 {
                map.insert("[omitted]".into(), json!("Header preview limit reached"));
                break;
            }
            map.insert(key.to_string(), Value::String(value));
        }
        Value::Object(map)
    }
    pub fn body(
        &self,
        value: Option<&Value>,
        bytes: usize,
        truncated: bool,
        limit: usize,
    ) -> Value {
        let value = value.map(|v| self.value(v));
        let fits = value
            .as_ref()
            .is_some_and(|v| serde_json::to_vec(v).is_ok_and(|v| v.len() <= limit));
        if fits && !truncated {
            json!({"value":value,"bytes":bytes,"truncated":false,"omitted":false})
        } else {
            json!({"value":null,"bytes":bytes,"truncated":truncated,"omitted":true,"reason":"Only complete JSON bodies within the preview limit are shown"})
        }
    }
    pub fn snapshot(&self, document: &interl_core::ScenarioDocument) -> Value {
        let mut snapshot = serde_json::to_value(document).expect("scenario is serializable");
        snapshot
            .as_object_mut()
            .expect("scenario is an object")
            .remove("layout");
        if let Some(vars) = snapshot
            .pointer_mut("/environment/variables")
            .and_then(Value::as_object_mut)
        {
            for v in vars.values_mut() {
                *v = json!(REDACTED);
            }
        }
        if let Some(profiles) = snapshot
            .get_mut("clientProfiles")
            .and_then(Value::as_array_mut)
        {
            for profile in profiles {
                if let Some(url) = profile.get_mut("baseUrl") {
                    *url = json!(self.url(url.as_str().unwrap_or_default()));
                }
            }
        }
        if let Some(nodes) = snapshot
            .pointer_mut("/workflow/nodes")
            .and_then(Value::as_array_mut)
        {
            for node in nodes {
                if let Some(path) = node.pointer_mut("/config/path") {
                    let path_text = path.as_str().unwrap_or_default();
                    *path = json!(self.text(path_text.split('?').next().unwrap_or_default()));
                }
                if let Some(body) = node.pointer_mut("/config/body") {
                    if body.get("kind").and_then(Value::as_str) == Some("text") {
                        body["value"] = json!("[Text body omitted]");
                    }
                }
            }
        }
        self.value(&snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_custom_fields_and_short_secrets_are_redacted_once() {
        let mut redactor = Redactor::new(vec!["credit-card".into()]);
        redactor.learn(&json!({"creditCard":"x","echo":"xxx"}));
        assert_eq!(
            redactor.value(&json!({"credit_card":"x","echo":"xxx"})),
            json!({"credit_card":"[REDACTED]","echo":"[REDACTED][REDACTED][REDACTED]"})
        );
    }
    #[test]
    fn exceeding_redaction_budget_omits_previews() {
        let mut redactor = Redactor::new(Vec::new());
        redactor.remember(&"s".repeat(1_048_577));
        assert_eq!(redactor.text("unknown"), "[REDACTED]");
        assert_eq!(
            redactor.value(&json!({"password":"unknown"})),
            json!("[Preview omitted: redaction budget exceeded]")
        );
    }
}
