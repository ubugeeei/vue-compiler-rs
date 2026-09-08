use std::path::Path;

use oxc_ast::ast::{Argument, ExportDefaultDeclarationKind, Expression};
use vize_carton::FxHashMap;
use vize_croquis::macros::PropDefinition;

use super::{object::collect_props_from_object, shape};
use crate::batch::virtual_project::setup_props::{
    RuntimePropResolveCache,
    imports::{RuntimeImport, RuntimePropVisitSet},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_props_from_default_declaration<'a>(
    declaration: &'a ExportDefaultDeclarationKind<'a>,
    source: &str,
    path: &Path,
    root_runtime_binding: &str,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    props: &mut Vec<PropDefinition>,
) {
    match declaration {
        ExportDefaultDeclarationKind::ObjectExpression(object) => collect_props_from_object(
            object,
            source,
            path,
            root_runtime_binding,
            imports,
            local_values,
            visited,
            cache,
            props,
        ),
        ExportDefaultDeclarationKind::CallExpression(call) => {
            if shape::is_runtime_props_wrapper(call)
                && let Some(first) = call.arguments.first()
            {
                collect_props_from_argument(
                    first,
                    source,
                    path,
                    root_runtime_binding,
                    imports,
                    local_values,
                    visited,
                    cache,
                    props,
                );
            }
        }
        ExportDefaultDeclarationKind::Identifier(identifier) => {
            if let Some(expr) = local_values.get(identifier.name.as_str()) {
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
            }
        }
        ExportDefaultDeclarationKind::TSAsExpression(ts_as) => collect_props_from_expression(
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
        ExportDefaultDeclarationKind::TSSatisfiesExpression(ts_satisfies) => {
            collect_props_from_expression(
                &ts_satisfies.expression,
                source,
                path,
                root_runtime_binding,
                imports,
                local_values,
                visited,
                cache,
                props,
            )
        }
        ExportDefaultDeclarationKind::TSNonNullExpression(ts_non_null) => {
            collect_props_from_expression(
                &ts_non_null.expression,
                source,
                path,
                root_runtime_binding,
                imports,
                local_values,
                visited,
                cache,
                props,
            )
        }
        ExportDefaultDeclarationKind::ParenthesizedExpression(parenthesized) => {
            collect_props_from_expression(
                &parenthesized.expression,
                source,
                path,
                root_runtime_binding,
                imports,
                local_values,
                visited,
                cache,
                props,
            )
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_props_from_expression<'a>(
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
        Expression::ObjectExpression(object) => collect_props_from_object(
            object,
            source,
            path,
            root_runtime_binding,
            imports,
            local_values,
            visited,
            cache,
            props,
        ),
        Expression::CallExpression(call) => {
            if shape::is_runtime_props_wrapper(call)
                && let Some(first) = call.arguments.first()
            {
                collect_props_from_argument(
                    first,
                    source,
                    path,
                    root_runtime_binding,
                    imports,
                    local_values,
                    visited,
                    cache,
                    props,
                );
            }
        }
        Expression::TSAsExpression(ts_as) => collect_props_from_expression(
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
        Expression::TSSatisfiesExpression(ts_satisfies) => collect_props_from_expression(
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
        Expression::TSNonNullExpression(ts_non_null) => collect_props_from_expression(
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
        Expression::ParenthesizedExpression(parenthesized) => collect_props_from_expression(
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

#[allow(clippy::too_many_arguments)]
fn collect_props_from_argument<'a>(
    arg: &'a Argument<'a>,
    source: &str,
    path: &Path,
    root_runtime_binding: &str,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    props: &mut Vec<PropDefinition>,
) {
    match arg {
        Argument::ObjectExpression(object) => collect_props_from_object(
            object,
            source,
            path,
            root_runtime_binding,
            imports,
            local_values,
            visited,
            cache,
            props,
        ),
        Argument::TSAsExpression(ts_as) => collect_props_from_expression(
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
        Argument::TSSatisfiesExpression(ts_satisfies) => collect_props_from_expression(
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
        Argument::TSNonNullExpression(ts_non_null) => collect_props_from_expression(
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
        Argument::ParenthesizedExpression(parenthesized) => collect_props_from_expression(
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
