use std::path::Path;

use oxc_ast::ast::{Expression, ObjectExpression, ObjectPropertyKind};
use vize_carton::{FxHashMap, ToCompactString};
use vize_croquis::macros::PropDefinition;

use super::{collect::collect_props_from_expression, resolve_imported_runtime_props, shape};
use crate::batch::virtual_project::setup_props::{
    RuntimePropResolveCache,
    imports::{RuntimeImport, RuntimePropVisitSet},
    syntax::{runtime_object_property_name, runtime_prop_shape_member_type},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_props_from_object<'a>(
    object: &'a ObjectExpression<'a>,
    source: &str,
    path: &Path,
    root_runtime_binding: &str,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    props: &mut Vec<PropDefinition>,
) {
    for property in &object.properties {
        match property {
            ObjectPropertyKind::ObjectProperty(property) => {
                let Some(name) = runtime_object_property_name(&property.key) else {
                    continue;
                };
                props.push(PropDefinition {
                    name: name.to_compact_string(),
                    prop_type: Some(runtime_prop_shape_member_type(root_runtime_binding, name)),
                    required: shape::runtime_prop_is_required(&property.value),
                    default_value: shape::runtime_prop_is_defaulted(&property.value, source)
                        .then(|| "undefined".into()),
                });
            }
            ObjectPropertyKind::SpreadProperty(spread) => collect_props_from_spread_expression(
                &spread.argument,
                source,
                path,
                root_runtime_binding,
                imports,
                local_values,
                visited,
                cache,
                props,
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_props_from_spread_expression<'a>(
    expr: &'a Expression<'a>,
    source: &str,
    path: &Path,
    root_runtime_binding: &str,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    props: &mut Vec<PropDefinition>,
) {
    match expr {
        Expression::Identifier(identifier) => {
            let name = identifier.name.as_str();
            if let Some(expr) = local_values.get(name) {
                collect_props_from_expression(
                    expr,
                    source,
                    path,
                    root_runtime_binding,
                    imports,
                    local_values,
                    visited,
                    cache,
                    props,
                );
                return;
            }
            if let Some(import) = imports.get(name) {
                props.extend(resolve_imported_runtime_props(
                    path,
                    import.source.as_str(),
                    import.imported.as_str(),
                    root_runtime_binding,
                    visited,
                    cache,
                ));
            }
        }
        Expression::CallExpression(call) => {
            shape::collect_props_from_string_array_helper_call(call, root_runtime_binding, props);
        }
        Expression::TSAsExpression(ts_as) => collect_props_from_spread_expression(
            &ts_as.expression,
            source,
            path,
            root_runtime_binding,
            imports,
            local_values,
            visited,
            cache,
            props,
        ),
        Expression::TSSatisfiesExpression(ts_satisfies) => collect_props_from_spread_expression(
            &ts_satisfies.expression,
            source,
            path,
            root_runtime_binding,
            imports,
            local_values,
            visited,
            cache,
            props,
        ),
        Expression::TSNonNullExpression(ts_non_null) => collect_props_from_spread_expression(
            &ts_non_null.expression,
            source,
            path,
            root_runtime_binding,
            imports,
            local_values,
            visited,
            cache,
            props,
        ),
        Expression::ParenthesizedExpression(parenthesized) => collect_props_from_spread_expression(
            &parenthesized.expression,
            source,
            path,
            root_runtime_binding,
            imports,
            local_values,
            visited,
            cache,
            props,
        ),
        _ => {}
    }
}
