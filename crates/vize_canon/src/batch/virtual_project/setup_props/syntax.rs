use oxc_ast::ast::{CallExpression, Expression, PropertyKey};
use vize_carton::{CompactString, String, cstr};

pub(super) fn runtime_arg_identifier(args: &str) -> Option<&str> {
    let trimmed = strip_wrapping_parentheses(args.trim());
    if is_ts_identifier(trimmed) {
        Some(trimmed)
    } else {
        None
    }
}

fn strip_wrapping_parentheses(mut value: &str) -> &str {
    loop {
        let trimmed = value.trim();
        if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
            return trimmed;
        }
        value = &trimmed[1..trimmed.len() - 1];
    }
}

fn is_ts_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

pub(super) fn call_expression_name<'a>(call: &'a CallExpression<'a>) -> Option<&'a str> {
    match &call.callee {
        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
        _ => None,
    }
}

pub(super) fn runtime_call_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::CallExpression(call) => call_expression_name(call),
        Expression::TSAsExpression(ts_as) => runtime_call_name(&ts_as.expression),
        Expression::TSSatisfiesExpression(ts_satisfies) => {
            runtime_call_name(&ts_satisfies.expression)
        }
        Expression::TSNonNullExpression(ts_non_null) => runtime_call_name(&ts_non_null.expression),
        Expression::ParenthesizedExpression(parenthesized) => {
            runtime_call_name(&parenthesized.expression)
        }
        _ => None,
    }
}

pub(super) fn runtime_object_property_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

pub(super) fn runtime_prop_shape_member_type(runtime_binding: &str, name: &str) -> CompactString {
    cstr!(
        "__RuntimePropShape<typeof {runtime_binding}>[{}]",
        ts_string_literal(name).as_str()
    )
}

fn ts_string_literal(value: &str) -> String {
    let mut literal = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            _ => literal.push(ch),
        }
    }
    literal.push('"');
    literal
}
