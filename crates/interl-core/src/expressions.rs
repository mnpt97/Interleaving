//! Compile a deliberately bounded FEEL property-access subset. No evaluation.
use dsntk_feel::{context::FeelContext, values::Value, FeelScope};
use dsntk_feel_parser::{parse_expression, AstNode};
use serde::Serialize;

pub const MAX_EXPRESSION_BYTES: usize = 4096;
pub const MAX_EXPRESSION_SEGMENTS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledExpression {
    body_path: Vec<String>,
}
impl CompiledExpression {
    pub fn body_path(&self) -> &[String] {
        &self.body_path
    }
}

pub(crate) struct ExpressionError {
    pub code: &'static str,
    pub message: &'static str,
    pub offset: usize,
}
fn error(code: &'static str, message: &'static str, offset: usize) -> ExpressionError {
    ExpressionError {
        code,
        message,
        offset,
    }
}

pub(crate) fn compile_expression(source: &str) -> Result<CompiledExpression, ExpressionError> {
    if source.len() > MAX_EXPRESSION_BYTES {
        return Err(error(
            "expressionLimit",
            "FEEL source must be at most 4096 bytes.",
            4096,
        ));
    }
    let mut names = Vec::new();
    let mut offset = 0;
    for part in source.split('.') {
        let name = part.trim_matches(|c: char| c.is_ascii_whitespace());
        let name_offset = offset + part.len()
            - part
                .trim_start_matches(|c: char| c.is_ascii_whitespace())
                .len();
        if names.len() == MAX_EXPRESSION_SEGMENTS || name.len() > 128 {
            return Err(error(
                "expressionLimit",
                "FEEL paths allow at most 32 names of at most 128 bytes each.",
                name_offset,
            ));
        }
        if name.is_empty() {
            return Err(error(
                "invalidExpression",
                "Expected a property name in a response.body path.",
                name_offset,
            ));
        }
        for (index, byte) in name.bytes().enumerate() {
            if !(byte.is_ascii_alphabetic() || byte == b'_' || (index > 0 && byte.is_ascii_digit()))
            {
                return Err(error("unsupportedExpression", "Only ASCII property names separated by dots are supported; operators, calls, filters, and list access are unavailable.", name_offset + index));
            }
        }
        if matches!(
            name,
            "and"
                | "or"
                | "not"
                | "if"
                | "then"
                | "else"
                | "for"
                | "return"
                | "some"
                | "every"
                | "satisfies"
                | "in"
                | "between"
                | "instance"
                | "of"
                | "function"
                | "external"
                | "true"
                | "false"
                | "null"
        ) {
            return Err(error(
                "unsupportedExpression",
                "Reserved FEEL words cannot be property names in the supported subset.",
                name_offset,
            ));
        }
        names.push(name);
        offset += part.len() + 1;
    }
    if names.len() < 3 || names[..2] != ["response", "body"] {
        return Err(error(
            "unsupportedExpression",
            "Extraction supports only response.body property access.",
            0,
        ));
    }
    // Scope-aware parsing is required: an empty scope can parse a dotted path
    // as a single name. Placeholders contain no runtime response or secrets.
    let mut value = Value::Null(None);
    for name in names.iter().rev() {
        let mut context = FeelContext::default();
        context.set_entry(&(*name).into(), value);
        value = Value::Context(context);
    }
    let Value::Context(context) = value else {
        return Err(error(
            "invalidExpression",
            "Could not construct FEEL parsing scope.",
            0,
        ));
    };
    let ast =
        parse_expression(&FeelScope::from(context), &names.join("."), false).map_err(|_| {
            error(
                "invalidExpression",
                "FEEL parser rejected the property-access expression.",
                0,
            )
        })?;
    let mut actual = Vec::new();
    collect(&ast, &mut actual, 1, &mut 0)?;
    if actual != names {
        return Err(error(
            "unsupportedExpression",
            "FEEL AST does not match the permitted property path.",
            0,
        ));
    }
    Ok(CompiledExpression {
        body_path: actual.into_iter().skip(2).collect(),
    })
}

fn collect(
    ast: &AstNode,
    names: &mut Vec<String>,
    depth: usize,
    count: &mut usize,
) -> Result<(), ExpressionError> {
    *count += 1;
    if depth > 32 || *count > 256 {
        return Err(error(
            "expressionLimit",
            "FEEL AST exceeds depth or node limits.",
            0,
        ));
    }
    match ast {
        AstNode::Name(name) => names.push(name.as_str().to_owned()),
        AstNode::Path(left, right) if matches!(**right, AstNode::Name(_)) => {
            collect(left, names, depth + 1, count)?;
            collect(right, names, depth + 1, count)?;
        }
        _ => {
            return Err(error(
                "unsupportedExpression",
                "Only FEEL property-access AST nodes are supported.",
                0,
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ast_allowlist_and_budgets_are_enforced() {
        assert!(collect(&AstNode::Boolean(true), &mut vec![], 1, &mut 0).is_err());
        let name = AstNode::Name("token".into());
        assert!(collect(&name, &mut vec![], 33, &mut 0).is_err());
        assert!(collect(&name, &mut vec![], 1, &mut 256).is_err());
        let unscoped =
            parse_expression(&FeelScope::default(), "response.body.token", false).unwrap();
        assert!(matches!(unscoped, AstNode::Name(_)));
        assert_eq!(
            compile_expression("response.body.token")
                .ok()
                .unwrap()
                .body_path(),
            ["token"]
        );
    }
}
