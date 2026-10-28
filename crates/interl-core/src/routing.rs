//! Bounded FEEL predicates over response metadata/body and explicitly bound scalar inputs.
//! Admission runs before FEEL parsing. Only the checked AST becomes executable IR;
//! the FEEL library never receives runtime data or credentials.
use crate::expressions::{compile_expression, ExpressionError};
use dsntk_feel::{context::FeelContext, values::Value as FeelValue, FeelScope};
use dsntk_feel_parser::{parse_expression, AstNode};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Immutable checked boolean expression. Runtime values are never retained here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPredicate(Predicate);
#[derive(Debug, Clone, PartialEq, Eq)]
enum Operand {
    Literal(Value),
    Path(Vec<String>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Predicate {
    Value(Operand),
    Compare(u8, Operand, Operand),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

/// Safe runtime predicate failures; messages never include a response or input value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PredicateError {
    /// Evaluation stopped at a cancellation boundary.
    #[error("Route evaluation cancelled.")]
    Cancelled,
    /// A referenced property or scalar input was absent or null.
    #[error("A route property or input is unavailable.")]
    Unavailable,
    /// A property was structured, unsafe, or incompatible with the comparison.
    #[error(
        "Route operands must have matching scalar types; ordering requires integers or strings."
    )]
    TypeMismatch,
}
impl Operand {
    fn value<'a>(
        &'a self,
        status: &'a Value,
        body: Option<&'a Value>,
        inputs: &'a BTreeMap<String, Value>,
    ) -> Result<&'a Value, PredicateError> {
        let value = match self {
            Self::Literal(value) => value,
            Self::Path(path) if path == &["response", "status"] => status,
            Self::Path(path) if path.first().is_some_and(|s| s == "inputs") => {
                inputs.get(&path[1]).ok_or(PredicateError::Unavailable)?
            }
            Self::Path(path) => {
                let mut value = body.ok_or(PredicateError::Unavailable)?;
                for part in &path[2..] {
                    value = value.get(part).ok_or(PredicateError::Unavailable)?;
                }
                value
            }
        };
        if value.is_null() {
            return Err(PredicateError::Unavailable);
        }
        if crate::scalar_type(value).is_none() {
            return Err(PredicateError::TypeMismatch);
        }
        Ok(value)
    }
}
impl CompiledPredicate {
    fn paths(&self) -> BTreeSet<&[String]> {
        fn collect<'a>(predicate: &'a Predicate, paths: &mut BTreeSet<&'a [String]>) {
            let mut operand = |value: &'a Operand| {
                if let Operand::Path(path) = value {
                    paths.insert(path);
                }
            };
            match predicate {
                Predicate::Value(value) => operand(value),
                Predicate::Compare(_, a, b) => {
                    operand(a);
                    operand(b);
                }
                Predicate::And(a, b) | Predicate::Or(a, b) => {
                    collect(a, paths);
                    collect(b, paths);
                }
            }
        }
        let mut paths = BTreeSet::new();
        collect(&self.0, &mut paths);
        paths
    }
    /// Whether the engine must retain and safely parse the selected response body.
    pub fn requires_body(&self) -> bool {
        self.paths()
            .iter()
            .any(|path| path.len() >= 2 && path[0] == "response" && path[1] == "body")
    }
    /// Named gateway bindings needed by this expression, without copying their names.
    pub fn input_names(&self) -> BTreeSet<&str> {
        self.paths()
            .iter()
            .filter(|path| path[0] == "inputs")
            .map(|path| path[1].as_str())
            .collect()
    }
    /// Check statically known operand types. Body property types remain runtime checks.
    /// Bound input types come from normal scoped scalar/dataflow validation.
    pub(crate) fn validate_input_types(
        &self,
        inputs: &BTreeMap<String, crate::PortType>,
    ) -> Result<(), ExpressionError> {
        fn kind(
            operand: &Operand,
            inputs: &BTreeMap<String, crate::PortType>,
        ) -> Option<crate::PortType> {
            match operand {
                Operand::Literal(value) => crate::scalar_type(value),
                Operand::Path(path) if path == &["response", "status"] => {
                    Some(crate::PortType::Integer)
                }
                Operand::Path(path) if path[0] == "inputs" => inputs.get(&path[1]).copied(),
                _ => None,
            }
        }
        fn check(predicate: &Predicate, inputs: &BTreeMap<String, crate::PortType>) -> bool {
            match predicate {
                Predicate::Value(value) => {
                    kind(value, inputs).is_none_or(|kind| kind == crate::PortType::Boolean)
                }
                Predicate::And(a, b) | Predicate::Or(a, b) => check(a, inputs) && check(b, inputs),
                Predicate::Compare(op, a, b) => {
                    let (a, b) = (kind(a, inputs), kind(b, inputs));
                    !(a.is_some() && b.is_some() && a != b
                        || *op > 1
                            && (a == Some(crate::PortType::Boolean)
                                || b == Some(crate::PortType::Boolean)))
                }
            }
        }
        if check(&self.0, inputs) {
            Ok(())
        } else {
            Err(error(0))
        }
    }
    pub(crate) fn storage_bytes(&self) -> usize {
        fn count(p: &Predicate) -> usize {
            match p {
                Predicate::And(a, b) | Predicate::Or(a, b) => {
                    std::mem::size_of::<Predicate>() + count(a) + count(b)
                }
                Predicate::Value(v) => std::mem::size_of::<Predicate>() + operand(v),
                Predicate::Compare(_, a, b) => {
                    std::mem::size_of::<Predicate>() + operand(a) + operand(b)
                }
            }
        }
        fn operand(o: &Operand) -> usize {
            match o {
                Operand::Path(p) => p
                    .iter()
                    .map(|s| s.len() + std::mem::size_of::<String>())
                    .sum(),
                Operand::Literal(v) => v.to_string().len(),
            }
        }
        count(&self.0)
    }
    /// Evaluate a status-only condition. `None` means cancellation or unavailable data.
    pub fn evaluate(&self, status: u16, interrupted: impl Fn() -> bool) -> Option<bool> {
        self.evaluate_context(status, None, &BTreeMap::new(), interrupted)
            .ok()
    }
    /// Evaluate bounded typed operands against one invocation's read-only context.
    /// Missing/null properties and type mismatches fail explicitly rather than becoming false.
    pub fn evaluate_context(
        &self,
        status: u16,
        body: Option<&Value>,
        inputs: &BTreeMap<String, Value>,
        interrupted: impl Fn() -> bool,
    ) -> Result<bool, PredicateError> {
        fn eval(
            p: &Predicate,
            status: &Value,
            body: Option<&Value>,
            inputs: &BTreeMap<String, Value>,
            stop: &impl Fn() -> bool,
        ) -> Result<bool, PredicateError> {
            if stop() {
                return Err(PredicateError::Cancelled);
            }
            match p {
                Predicate::Value(value) => value
                    .value(status, body, inputs)?
                    .as_bool()
                    .ok_or(PredicateError::TypeMismatch),
                Predicate::And(a, b) => Ok(
                    eval(a, status, body, inputs, stop)? && eval(b, status, body, inputs, stop)?
                ),
                Predicate::Or(a, b) => Ok(
                    eval(a, status, body, inputs, stop)? || eval(b, status, body, inputs, stop)?
                ),
                Predicate::Compare(op, a, b) => {
                    let a = a.value(status, body, inputs)?;
                    let b = b.value(status, body, inputs)?;
                    if crate::scalar_type(a) != crate::scalar_type(b) {
                        return Err(PredicateError::TypeMismatch);
                    }
                    let order = if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                        Some(a.cmp(&b))
                    } else if let (Some(a), Some(b)) = (a.as_str(), b.as_str()) {
                        Some(a.cmp(b))
                    } else {
                        None
                    };
                    Ok(match op {
                        0 => a == b,
                        1 => a != b,
                        2 => order.ok_or(PredicateError::TypeMismatch)?.is_lt(),
                        3 => !order.ok_or(PredicateError::TypeMismatch)?.is_gt(),
                        4 => order.ok_or(PredicateError::TypeMismatch)?.is_gt(),
                        _ => !order.ok_or(PredicateError::TypeMismatch)?.is_lt(),
                    })
                }
            }
        }
        eval(&self.0, &Value::from(status), body, inputs, &interrupted)
    }
}
fn error(offset: usize) -> ExpressionError {
    ExpressionError { code: "invalidRouteCondition", message: "Use bounded FEEL comparisons of response.status, response.body properties, or inputs with scalar literals.", offset }
}
fn limit(offset: usize) -> ExpressionError {
    ExpressionError {
        code: "expressionLimit",
        message: "Route predicate exceeds byte, token, depth, AST, or comparison limits.",
        offset,
    }
}

/// Compile read-only FEEL scalar comparisons and boolean combinations.
/// # Errors
/// Rejects unsupported syntax, unsafe integers, unknown namespaces, or resource limits.
pub fn compile_predicate(source: &str) -> Result<CompiledPredicate, ExpressionError> {
    if source.len() > 4096 {
        return Err(limit(4096));
    }
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if bytes[i] == b'"' {
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b'"' {
                    i += 1;
                    closed = true;
                    break;
                } else {
                    i += 1;
                }
            }
            if !closed || i > bytes.len() {
                return Err(error(start));
            }
        } else if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        } else {
            if !bytes[i].is_ascii() {
                return Err(error(start));
            }
            i += 1;
            if matches!(bytes[start], b'!' | b'<' | b'>') && bytes.get(i) == Some(&b'=') {
                i += 1;
            }
        }
        tokens.push((&source[start..i], start));
        if tokens.len() > 512 {
            return Err(limit(start));
        }
    }
    struct Gate<'a> {
        tokens: Vec<(&'a str, usize)>,
        at: usize,
        nodes: usize,
        comparisons: usize,
    }
    impl Gate<'_> {
        fn take(&mut self, t: &str) -> bool {
            if self.tokens.get(self.at).is_some_and(|v| v.0 == t) {
                self.at += 1;
                true
            } else {
                false
            }
        }
        fn operand(&mut self) -> Result<Operand, ExpressionError> {
            let &(token, offset) = self.tokens.get(self.at).ok_or_else(|| error(0))?;
            if token == "true" || token == "false" {
                self.at += 1;
                return Ok(Operand::Literal(Value::Bool(token == "true")));
            }
            if token.starts_with('"') {
                self.at += 1;
                return serde_json::from_str(token)
                    .map(Operand::Literal)
                    .map_err(|_| error(offset));
            }
            if token == "-" || token.bytes().all(|b| b.is_ascii_digit()) {
                let negative = self.take("-");
                let &(digits, _) = self.tokens.get(self.at).ok_or_else(|| error(offset))?;
                if digits.len() > 16
                    || !digits.bytes().all(|b| b.is_ascii_digit())
                    || (digits.len() > 1 && digits.starts_with('0'))
                {
                    return Err(error(offset));
                }
                let value = digits.parse::<i64>().map_err(|_| error(offset))?;
                if value > 9_007_199_254_740_991 {
                    return Err(error(offset));
                }
                self.at += 1;
                return Ok(Operand::Literal(Value::from(if negative {
                    -value
                } else {
                    value
                })));
            }
            let mut path = Vec::new();
            loop {
                let &(name, at) = self.tokens.get(self.at).ok_or_else(|| error(offset))?;
                if name.len() > 128
                    || !name.bytes().enumerate().all(|(i, b)| {
                        b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit())
                    })
                {
                    return Err(error(at));
                }
                path.push(name.to_owned());
                self.at += 1;
                if path.len() > 32 {
                    return Err(limit(at));
                }
                if !self.take(".") {
                    break;
                }
            }
            if path.len() >= 2 && path[0] == "response" && path[1] == "body" {
                compile_expression(&path.join(".")).map_err(|_| error(offset))?;
            } else if !(path == ["response", "status"] || (path.len() == 2 && path[0] == "inputs"))
            {
                return Err(error(offset));
            }
            Ok(Operand::Path(path))
        }
        fn expr(&mut self, depth: usize, and: bool) -> Result<Predicate, ExpressionError> {
            let mut p = if and {
                self.primary(depth)?
            } else {
                self.expr(depth, true)?
            };
            while self.take(if and { "and" } else { "or" }) {
                self.nodes += 1;
                if self.nodes > 256 {
                    return Err(limit(0));
                }
                let q = if and {
                    self.primary(depth)?
                } else {
                    self.expr(depth, true)?
                };
                p = if and {
                    Predicate::And(Box::new(p), Box::new(q))
                } else {
                    Predicate::Or(Box::new(p), Box::new(q))
                };
            }
            Ok(p)
        }
        fn primary(&mut self, depth: usize) -> Result<Predicate, ExpressionError> {
            self.nodes += 1;
            if depth > 30 || self.nodes > 256 {
                return Err(limit(0));
            }
            if self.take("(") {
                let p = self.expr(depth + 1, false)?;
                if !self.take(")") {
                    return Err(error(0));
                }
                return Ok(p);
            }
            let a = self.operand()?;
            if let Some(op) = ["=", "!=", "<", "<=", ">", ">="]
                .iter()
                .position(|op| self.take(op))
            {
                self.comparisons += 1;
                if self.comparisons > 64 {
                    return Err(limit(0));
                }
                Ok(Predicate::Compare(op as u8, a, self.operand()?))
            } else {
                Ok(Predicate::Value(a))
            }
        }
    }
    let mut gate = Gate {
        tokens,
        at: 0,
        nodes: 0,
        comparisons: 0,
    };
    let admitted = gate.expr(1, false)?;
    if gate.at != gate.tokens.len() {
        return Err(error(gate.tokens[gate.at].1));
    }
    fn depth(p: &Predicate) -> usize {
        match p {
            Predicate::And(a, b) | Predicate::Or(a, b) => 1 + depth(a).max(depth(b)),
            _ => 1,
        }
    }
    if depth(&admitted) > 30 {
        return Err(limit(0));
    }
    let compiled = CompiledPredicate(admitted);
    // Scope placeholders describe only admitted property names, never runtime values.
    let mut shape = serde_json::Map::new();
    for path in compiled.paths() {
        let mut fields = &mut shape;
        for (index, part) in path.iter().enumerate() {
            let entry = fields
                .entry(part.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if index + 1 == path.len() {
                break;
            }
            fields = entry.as_object_mut().ok_or_else(|| error(0))?;
        }
    }
    fn scope(fields: &serde_json::Map<String, Value>) -> FeelContext {
        let mut context = FeelContext::default();
        for (key, value) in fields {
            context.set_entry(
                &key.as_str().into(),
                if let Some(fields) = value.as_object().filter(|f| !f.is_empty()) {
                    FeelValue::Context(scope(fields))
                } else {
                    FeelValue::Null(None)
                },
            );
        }
        context
    }
    let ast =
        parse_expression(&FeelScope::from(scope(&shape)), source, false).map_err(|_| error(0))?;
    fn operand(ast: &AstNode, depth: usize, count: &mut usize) -> Result<Operand, ExpressionError> {
        *count += 1;
        if depth > 32 || *count > 256 {
            return Err(limit(0));
        }
        match ast {
            AstNode::Boolean(value) => Ok(Operand::Literal(Value::Bool(*value))),
            AstNode::String(value) => Ok(Operand::Literal(Value::String(value.clone()))),
            AstNode::Numeric(digits, _, _, _) => serde_json::from_str(digits)
                .map(Operand::Literal)
                .map_err(|_| error(0)),
            AstNode::Neg(value) => {
                let Operand::Literal(Value::Number(number)) = operand(value, depth + 1, count)?
                else {
                    return Err(error(0));
                };
                Ok(Operand::Literal(Value::from(
                    -number.as_i64().ok_or_else(|| error(0))?,
                )))
            }
            AstNode::Name(name) => Ok(Operand::Path(vec![name.as_str().to_owned()])),
            AstNode::Path(a, b) => {
                let Operand::Path(mut a) = operand(a, depth + 1, count)? else {
                    return Err(error(0));
                };
                let Operand::Path(b) = operand(b, depth + 1, count)? else {
                    return Err(error(0));
                };
                a.extend(b);
                Ok(Operand::Path(a))
            }
            _ => Err(error(0)),
        }
    }
    fn convert(
        ast: &AstNode,
        depth: usize,
        count: &mut usize,
    ) -> Result<Predicate, ExpressionError> {
        *count += 1;
        if depth > 32 || *count > 256 {
            return Err(limit(0));
        }
        match ast {
            AstNode::And(a, b) | AstNode::Or(a, b) => {
                let a = Box::new(convert(a, depth + 1, count)?);
                let b = Box::new(convert(b, depth + 1, count)?);
                Ok(if matches!(ast, AstNode::And(..)) {
                    Predicate::And(a, b)
                } else {
                    Predicate::Or(a, b)
                })
            }
            AstNode::Eq(a, b)
            | AstNode::Nq(a, b)
            | AstNode::Lt(a, b)
            | AstNode::Le(a, b)
            | AstNode::Gt(a, b)
            | AstNode::Ge(a, b) => {
                let op = match ast {
                    AstNode::Eq(..) => 0,
                    AstNode::Nq(..) => 1,
                    AstNode::Lt(..) => 2,
                    AstNode::Le(..) => 3,
                    AstNode::Gt(..) => 4,
                    _ => 5,
                };
                Ok(Predicate::Compare(
                    op,
                    operand(a, depth + 1, count)?,
                    operand(b, depth + 1, count)?,
                ))
            }
            _ => Ok(Predicate::Value(operand(ast, depth + 1, count)?)),
        }
    }
    if convert(&ast, 1, &mut 0)? != compiled.0 {
        return Err(error(0));
    }
    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn predicates_compare_status_body_and_bound_scalars_without_coercion() {
        let body = json!({"value":{"status":true,"attempts":3,"label":"ready"}});
        let inputs = BTreeMap::from([("ready".into(), json!(true))]);
        for expression in [
            "response.status = 200",
            "response.body.value.status = true",
            "response.body.value.status = inputs.ready",
            "response.body.value.attempts >= 2 and response.body.value.label = \"ready\"",
            "response.body.value.status",
            "response.body.value.attempts > -1",
        ] {
            let predicate =
                compile_predicate(expression).unwrap_or_else(|e| panic!("{expression}: {e}"));
            assert_eq!(
                predicate.evaluate_context(200, Some(&body), &inputs, || false),
                Ok(true),
                "{expression}"
            );
        }
        assert_eq!(
            compile_predicate("response.body.value.status = \"true\"")
                .unwrap()
                .evaluate_context(200, Some(&body), &inputs, || false),
            Err(PredicateError::TypeMismatch)
        );
    }
    #[test]
    fn unavailable_null_structured_and_cancelled_values_fail_without_exposing_data() {
        for value in [json!({}), json!({"flag":null}), json!({"flag":{}})] {
            assert!(compile_predicate("response.body.flag = true")
                .unwrap()
                .evaluate_context(200, Some(&value), &BTreeMap::new(), || false)
                .is_err());
        }
        assert_eq!(
            compile_predicate("true").unwrap().evaluate_context(
                200,
                None,
                &BTreeMap::new(),
                || true
            ),
            Err(PredicateError::Cancelled)
        );
        assert_eq!(
            compile_predicate("false and response.body.missing = true")
                .unwrap()
                .evaluate_context(200, Some(&json!({})), &BTreeMap::new(), || false),
            Ok(false)
        );
    }
    #[test]
    fn parser_admission_rejects_general_evaluation_and_excessive_resources() {
        for expression in [
            "response.password = 200",
            "response.body.x() = true",
            "response.status = 020",
            "response.status = 2.0",
            "response.status = 9007199254740992",
            "not(true)",
            "true + false",
            "response.status = 2e2",
            "",
            "é",
        ] {
            assert!(compile_predicate(expression).is_err(), "{expression}");
        }
        assert!(compile_predicate(&format!("{}true{}", "(".repeat(33), ")".repeat(33))).is_err());
        assert!(compile_predicate(&"true or ".repeat(600)).is_err());
    }
}
