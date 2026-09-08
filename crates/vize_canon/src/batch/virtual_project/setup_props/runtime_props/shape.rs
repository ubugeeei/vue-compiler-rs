use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectExpression, ObjectPropertyKind};
use oxc_span::GetSpan;
use vize_carton::ToCompactString;
use vize_croquis::macros::PropDefinition;

use crate::batch::virtual_project::setup_props::syntax::{
    runtime_call_name, runtime_object_property_name, runtime_prop_shape_member_type,
};

pub(super) fn is_runtime_props_wrapper(call: &CallExpression<'_>) -> bool {
    match &call.callee {
        Expression::Identifier(identifier) => {
            matches!(identifier.name.as_str(), "buildProps" | "defineProps")
        }
        _ => false,
    }
}

pub(super) fn runtime_prop_is_required(expr: &Expression<'_>) -> bool {
    let Some(object) = runtime_prop_options_object(expr) else {
        return false;
    };
    object.properties.iter().any(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return false;
        };
        runtime_object_property_name(&property.key) == Some("required")
            && matches!(&property.value, Expression::BooleanLiteral(value) if value.value)
    })
}

pub(super) fn runtime_prop_is_defaulted(expr: &Expression<'_>, source: &str) -> bool {
    if runtime_prop_declares_boolean(expr) {
        return true;
    }
    let Some(object) = runtime_prop_options_object(expr) else {
        return runtime_call_name(expr).is_some_and(|name| name.ends_with("WithDefault"));
    };
    object.properties.iter().any(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return false;
        };
        runtime_object_property_name(&property.key) == Some("default")
            || (runtime_object_property_name(&property.key) == Some("type")
                && runtime_prop_declares_boolean(&property.value))
            || property
                .key
                .span()
                .source_text(source)
                .trim()
                .ends_with("WithDefault")
    })
}

pub(super) fn collect_props_from_string_array_helper_call(
    call: &CallExpression<'_>,
    root_runtime_binding: &str,
    props: &mut Vec<PropDefinition>,
) {
    let Expression::Identifier(callee) = &call.callee else {
        return;
    };
    if callee.name.as_str() != "useAriaProps" {
        return;
    }
    let Some(Argument::ArrayExpression(names)) = call.arguments.first() else {
        return;
    };
    for element in &names.elements {
        let oxc_ast::ast::ArrayExpressionElement::StringLiteral(name) = element else {
            continue;
        };
        let name = name.value.as_str();
        props.push(PropDefinition {
            name: name.to_compact_string(),
            prop_type: Some(runtime_prop_shape_member_type(root_runtime_binding, name)),
            required: false,
            default_value: None,
        });
    }
}

fn runtime_prop_options_object<'a>(expr: &'a Expression<'a>) -> Option<&'a ObjectExpression<'a>> {
    match expr {
        Expression::ObjectExpression(object) => Some(object),
        Expression::TSAsExpression(ts_as) => runtime_prop_options_object(&ts_as.expression),
        Expression::TSSatisfiesExpression(ts_satisfies) => {
            runtime_prop_options_object(&ts_satisfies.expression)
        }
        Expression::TSNonNullExpression(ts_non_null) => {
            runtime_prop_options_object(&ts_non_null.expression)
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            runtime_prop_options_object(&parenthesized.expression)
        }
        _ => None,
    }
}

fn runtime_prop_declares_boolean(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::Identifier(identifier) => identifier.name.as_str() == "Boolean",
        Expression::ArrayExpression(array) => array.elements.iter().any(|element| match element {
            oxc_ast::ast::ArrayExpressionElement::Identifier(identifier) => {
                identifier.name.as_str() == "Boolean"
            }
            oxc_ast::ast::ArrayExpressionElement::SpreadElement(spread) => {
                runtime_prop_declares_boolean(&spread.argument)
            }
            _ => false,
        }),
        Expression::ObjectExpression(object) => object.properties.iter().any(|property| {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return false;
            };
            runtime_object_property_name(&property.key) == Some("type")
                && runtime_prop_declares_boolean(&property.value)
        }),
        Expression::TSAsExpression(ts_as) => runtime_prop_declares_boolean(&ts_as.expression),
        Expression::TSSatisfiesExpression(ts_satisfies) => {
            runtime_prop_declares_boolean(&ts_satisfies.expression)
        }
        Expression::TSNonNullExpression(ts_non_null) => {
            runtime_prop_declares_boolean(&ts_non_null.expression)
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            runtime_prop_declares_boolean(&parenthesized.expression)
        }
        _ => false,
    }
}
