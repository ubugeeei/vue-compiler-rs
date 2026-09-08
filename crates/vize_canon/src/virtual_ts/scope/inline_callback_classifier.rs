//! Direct inline-function classification for generic component props.

use oxc_allocator::Allocator;
use oxc_ast::ast::Expression;
use oxc_parser::Parser;
use oxc_span::SourceType;

use super::expression_scanner::{has_top_level_comma, skip_js_trivia};
use super::handler_shape::inline_callback_event_argument;

/// Whether the value itself is a callback, rather than an expression that only
/// contains one (for example `items.map((item) => item.id)`).
pub(crate) fn is_direct_inline_function_prop_value(value: &str) -> bool {
    inline_callback_event_argument(value).is_some() || parsed_direct_inline_function(value)
}

/// Fall back to the TypeScript parser only for callback-shaped values the
/// allocation-free scanner cannot settle. JavaScript's regex/division lexical
/// goal depends on full grammar context, so duplicating it here would make
/// rare valid defaults less reliable than the parser already used by Canon.
fn parsed_direct_inline_function(value: &str) -> bool {
    let value = value.trim();
    if !value.contains("=>") || !looks_like_inline_callback_start(value) {
        return false;
    }
    let allocator = Allocator::new();
    let Ok(expression) = Parser::new(&allocator, value, SourceType::ts()).parse_expression() else {
        return false;
    };
    expression_is_inline_function(&expression)
}

fn looks_like_inline_callback_start(value: &str) -> bool {
    if value.starts_with(['(', '<'])
        || value.starts_with("async")
        || value.contains('?')
        || has_top_level_comma(value)
    {
        return true;
    }
    let mut end = 0;
    for (index, ch) in value.char_indices() {
        if index == 0 {
            if !(ch == '_' || ch == '$' || ch.is_alphabetic()) {
                return false;
            }
        } else if !(ch == '_' || ch == '$' || ch.is_alphanumeric()) {
            break;
        }
        end = index + ch.len_utf8();
    }
    let after_identifier = skip_js_trivia(value, end);
    value[after_identifier..].starts_with("=>")
}

fn expression_is_inline_function(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => true,
        Expression::ParenthesizedExpression(parenthesized) => {
            expression_is_inline_function(&parenthesized.expression)
        }
        Expression::TSAsExpression(assertion) => {
            expression_is_inline_function(&assertion.expression)
        }
        Expression::TSSatisfiesExpression(assertion) => {
            expression_is_inline_function(&assertion.expression)
        }
        Expression::TSTypeAssertion(assertion) => {
            expression_is_inline_function(&assertion.expression)
        }
        Expression::TSNonNullExpression(non_null) => {
            expression_is_inline_function(&non_null.expression)
        }
        Expression::ConditionalExpression(conditional) => {
            expression_is_inline_function(&conditional.consequent)
                && expression_is_inline_function(&conditional.alternate)
        }
        Expression::SequenceExpression(sequence) => sequence
            .expressions
            .last()
            .is_some_and(expression_is_inline_function),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_direct_inline_function_prop_value;

    #[test]
    fn distinguishes_direct_callbacks_from_nested_callbacks() {
        let direct = [
            "(fn: (x: string) => string) => fn('x')",
            r"(pattern = /* default */ /\)/, value) /* callback */ => value",
            "(pattern = foo++ / bar, value) => value",
            "(pattern = foo! / bar, value) => value",
            r"(fn = () => { if (ok) /\)/.test(value) }, value) => value",
            "value /* callback */ => value",
            "ok ? (value) => value : (value) => value",
            "(0, (value) => value)",
            "void 0, (value) => value",
        ];
        for value in direct {
            assert!(
                is_direct_inline_function_prop_value(value),
                "expected a direct callback: {value}"
            );
        }

        let nested = [
            "jobs.map((job) => job.id)",
            "[() => true]",
            "{ apply: (value) => value }",
            "ok ? (value) => value : fallback",
            "(0, jobs.map((job) => job.id))",
            "void 0, jobs.map((job) => job.id)",
            "/=>/.test(value)",
        ];
        for value in nested {
            assert!(
                !is_direct_inline_function_prop_value(value),
                "expected only a nested callback: {value}"
            );
        }
    }
}
