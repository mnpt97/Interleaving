use super::*;
enum ScalarRef<'a> {
    Value(&'a Value),
    String(&'a str),
}
impl ScalarRef<'_> {
    fn bytes(&self) -> usize {
        match self {
            Self::Value(v) => serde_json::to_vec(v).expect("scalar").len(),
            Self::String(s) => serde_json::to_vec(s).expect("string scalar").len(),
        }
    }
    fn owned(&self) -> Value {
        match self {
            Self::Value(v) => (*v).clone(),
            Self::String(s) => json!(s),
        }
    }
}
impl Runtime {
    pub(super) fn prepare_http(
        &self,
        frame: &Frame,
        http: &ProgramHttp,
        cancel: &CancellationToken,
    ) -> Result<interl_http::PreparedRequest, RunFailure> {
        check(cancel)?;
        let cfg = static_config(http);
        let request = match &http.body {
            Some(ProgramBody::JsonTemplate { fields }) => {
                let mut count = 1;
                let body = fields
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            key.clone(),
                            self.template(value, frame, 2, &mut count, cancel)?,
                        ))
                    })
                    .collect::<Result<serde_json::Map<_, _>, RunFailure>>()?;
                self.session.prepare_resolved(
                    &cfg,
                    &self.program.document().environment.variables,
                    ResolvedBody::Json(Value::Object(body)),
                )?
            }
            _ => self
                .session
                .prepare(&cfg, &self.program.document().environment.variables)?,
        };
        check(cancel)?;
        Ok(request)
    }

    fn scalar<'a>(
        &'a self,
        source: &'a ScalarSource,
        frame: Option<&'a Frame>,
        init: Option<&'a BTreeMap<String, Value>>,
    ) -> Result<ScalarRef<'a>, RunFailure> {
        let value = match source {
            ScalarSource::Literal { value } => Some(value),
            ScalarSource::Environment { name } => {
                self.program.document().environment.variables.get(name)
            }
            ScalarSource::Identity { field } => {
                let interl_core::IdentityAssignment::Shared { identity_id } =
                    &self.program.document().groups[0].identity_assignment;
                return identity_id
                    .as_deref()
                    .and_then(|id| self.security.credential(id, field))
                    .map(ScalarRef::String)
                    .ok_or_else(|| {
                        failure(
                            "identityCredentialUnavailable",
                            "Assigned credential is unavailable.",
                        )
                    });
            }
            ScalarSource::Lifecycle { name, .. } => init.and_then(|m| m.get(name)),
            ScalarSource::Extraction { node_id, name } => {
                return frame
                    .ok_or_else(|| failure("valueNotAvailable", "Invocation source unavailable."))?
                    .extraction
                    .output(node_id, name)
                    .map(|v| ScalarRef::String(v.expose()))
                    .map_err(Into::into)
            }
            _ => source
                .key()
                .and_then(|key| frame.and_then(|f| f.values.get(&key))),
        };
        value
            .map(ScalarRef::Value)
            .ok_or_else(|| failure("valueNotAvailable", "Required scalar value is unavailable."))
    }
    pub(super) fn extraction_budget(&self, frame: &Frame) -> Result<(usize, usize), RunFailure> {
        let mut count = 0;
        let mut bytes = 0;
        for f in self.frames.iter().chain(std::iter::once(frame)) {
            let (n, b, _) = f.extraction.usage();
            count += n + f.values.len();
            bytes += b + f
                .values
                .values()
                .map(|v| serde_json::to_vec(v).expect("scalar").len())
                .sum::<usize>();
        }
        if count > 128 || bytes > 65536 {
            return Err(failure(
                "valueLimit",
                "Aggregate invocation storage exceeds its resource budget.",
            ));
        }
        let (n, b, _) = frame.extraction.usage();
        Ok((n + 128 - count, b + 65536 - bytes))
    }
    pub(super) fn capacity(
        &self,
        entries: usize,
        bytes: usize,
        responses: usize,
    ) -> Result<(), RunFailure> {
        let mut count = entries;
        let mut total = bytes;
        let mut body = responses;
        for f in &self.frames {
            let (n, b, r) = f.extraction.usage();
            count += n + f.values.len();
            total += b + f
                .values
                .values()
                .map(|v| serde_json::to_vec(v).expect("scalar").len())
                .sum::<usize>();
            body += r;
        }
        if count > 128 || total > 65536 || body > 1048576 {
            Err(failure(
                "valueLimit",
                "Aggregate invocation storage exceeds its resource budget.",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn bindings(
        &mut self,
        binding: &WorkflowBinding,
        parent: Option<&Frame>,
        init: Option<&BTreeMap<String, Value>>,
    ) -> Result<BTreeMap<String, Value>, RunFailure> {
        let flow = self
            .program
            .workflow_index(&binding.workflow_id)
            .expect("validated workflow");
        let ports = &self.program.workflows()[flow].definition().inputs;
        let mut values = BTreeMap::new();
        let mut bytes = init.map_or(0, |m| {
            m.values()
                .map(|v| serde_json::to_vec(v).expect("scalar").len())
                .sum()
        });
        let mut base = init.map_or(0, BTreeMap::len);
        let mut responses = 0;
        if let Some(p) = parent {
            let (n, b, r) = p.extraction.usage();
            base += n + p.values.len();
            bytes += b + p
                .values
                .values()
                .map(|v| serde_json::to_vec(v).expect("scalar").len())
                .sum::<usize>();
            responses += r;
        }
        for (name, port) in ports {
            let value = if let Some(source) = binding.inputs.get(name) {
                self.scalar(source, parent, init)?
            } else {
                ScalarRef::Value(
                    port.default
                        .as_ref()
                        .ok_or_else(|| failure("missingInput", "Workflow input is missing."))?,
                )
            };
            self.capacity(base + values.len() + 1, bytes + value.bytes(), responses)?;
            let owned = value.owned();
            if scalar_type(&owned) != Some(port.kind) {
                return Err(failure(
                    "portTypeMismatch",
                    "Workflow input has an invalid scalar type or size.",
                ));
            }
            bytes += value.bytes();
            values.insert(name.clone(), owned);
        }
        for value in values.values() {
            if let Some(s) = value.as_str() {
                self.security.remember_secret(s);
            }
        }
        Ok(values)
    }
    pub(super) fn outputs(&mut self, frame: &Frame) -> Result<BTreeMap<String, Value>, RunFailure> {
        let mut outputs = BTreeMap::new();
        let mut bytes = 0;
        for (name, port) in &self.program.workflows()[frame.flow].definition().outputs {
            let value = self.scalar(&port.value, Some(frame), None)?;
            let (n, b, r) = frame.extraction.usage();
            self.capacity(
                outputs.len() + n + frame.values.len() + 1,
                bytes
                    + b
                    + frame
                        .values
                        .values()
                        .map(|v| serde_json::to_vec(v).expect("scalar").len())
                        .sum::<usize>()
                    + value.bytes(),
                r,
            )?;
            let owned = value.owned();
            if scalar_type(&owned) != Some(port.kind) {
                return Err(failure(
                    "portTypeMismatch",
                    "Workflow output has an invalid scalar type or size.",
                ));
            }
            bytes += value.bytes();
            outputs.insert(name.clone(), owned);
        }
        for value in outputs.values() {
            if let Some(s) = value.as_str() {
                self.security.remember_secret(s);
            }
        }
        Ok(outputs)
    }
    pub(super) fn template(
        &self,
        value: &ProgramTemplate,
        frame: &Frame,
        depth: usize,
        nodes: &mut usize,
        cancel: &CancellationToken,
    ) -> Result<Value, RunFailure> {
        check(cancel)?;
        *nodes += 1;
        if depth > 32 || *nodes > 256 {
            return Err(failure(
                "templateLimit",
                "Template composition exceeds runtime limits.",
            ));
        }
        Ok(match value {
            ProgramTemplate::Literal { value } => value.clone(),
            ProgramTemplate::Object { fields } => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| {
                        Ok((
                            k.clone(),
                            self.template(v, frame, depth + 1, nodes, cancel)?,
                        ))
                    })
                    .collect::<Result<_, RunFailure>>()?,
            ),
            ProgramTemplate::Array { items } => Value::Array(
                items
                    .iter()
                    .map(|v| self.template(v, frame, depth + 1, nodes, cancel))
                    .collect::<Result<_, _>>()?,
            ),
            _ => self
                .scalar(&value.scalar().expect("scalar template"), Some(frame), None)?
                .owned(),
        })
    }
    pub(super) fn session_update(
        &mut self,
        config: &ProgramSession,
        frame: &Frame,
        cancel: &CancellationToken,
    ) -> Result<(), RunFailure> {
        check(cancel)?;
        let mut set = BTreeMap::new();
        for (name, h) in &config.set_headers {
            let (source, prefix) = h.parts();
            let scalar = self.scalar(&source, Some(frame), None)?.owned();
            let value = scalar.as_str().ok_or_else(|| {
                failure(
                    "portTypeMismatch",
                    "Session header sources require strings.",
                )
            })?;
            if prefix.len() + value.len() > 8192 {
                return Err(failure(
                    "sessionHeaderLimit",
                    "Rendered header exceeds 8192 bytes.",
                ));
            }
            set.insert(name.clone(), format!("{prefix}{value}"));
        }
        check(cancel)?;
        self.session.update_headers(&set, &config.remove_headers)?;
        for value in set.values() {
            self.security.remember_secret(value);
        }
        Ok(())
    }
}
