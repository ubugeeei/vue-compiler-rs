use oxc_ast::ast::{Argument, CallExpression, Expression};
use vize_carton::{CompactString, FxHashSet, ToCompactString};

pub(super) fn collect_omit_key_arguments(call: &CallExpression<'_>) -> FxHashSet<CompactString> {
    let mut names = FxHashSet::default();
    for arg in call.arguments.iter().skip(1) {
        collect_string_literal_names_from_argument(arg, &mut names);
    }
    names
}

pub(super) fn default_value_is_undefined(value: &Expression<'_>) -> bool {
    matches!(value, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

fn collect_string_literal_names_from_argument(
    arg: &Argument<'_>,
    names: &mut FxHashSet<CompactString>,
) {
    match arg {
        Argument::StringLiteral(value) => {
            names.insert(value.value.to_compact_string());
        }
        Argument::ArrayExpression(array) => {
            for element in &array.elements {
                if let oxc_ast::ast::ArrayExpressionElement::StringLiteral(value) = element {
                    names.insert(value.value.to_compact_string());
                }
            }
        }
        Argument::TSAsExpression(ts_as) => {
            collect_string_literal_names_from_expression(&ts_as.expression, names);
        }
        Argument::TSSatisfiesExpression(ts_satisfies) => {
            collect_string_literal_names_from_expression(&ts_satisfies.expression, names);
        }
        Argument::TSNonNullExpression(ts_non_null) => {
            collect_string_literal_names_from_expression(&ts_non_null.expression, names);
        }
        Argument::ParenthesizedExpression(parenthesized) => {
            collect_string_literal_names_from_expression(&parenthesized.expression, names);
        }
        _ => {}
    }
}

fn collect_string_literal_names_from_expression(
    expr: &Expression<'_>,
    names: &mut FxHashSet<CompactString>,
) {
    match expr {
        Expression::StringLiteral(value) => {
            names.insert(value.value.to_compact_string());
        }
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                if let oxc_ast::ast::ArrayExpressionElement::StringLiteral(value) = element {
                    names.insert(value.value.to_compact_string());
                }
            }
        }
        Expression::TSAsExpression(ts_as) => {
            collect_string_literal_names_from_expression(&ts_as.expression, names);
        }
        Expression::TSSatisfiesExpression(ts_satisfies) => {
            collect_string_literal_names_from_expression(&ts_satisfies.expression, names);
        }
        Expression::TSNonNullExpression(ts_non_null) => {
            collect_string_literal_names_from_expression(&ts_non_null.expression, names);
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            collect_string_literal_names_from_expression(&parenthesized.expression, names);
        }
        _ => {}
    }
}
