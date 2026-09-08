use std::path::Path;

use oxc_ast::ast::{ExportDefaultDeclarationKind, Expression};
use vize_carton::{CompactString, FxHashMap, FxHashSet};

use super::collect::{
    collect_default_names_from_call, collect_default_names_from_expression,
    collect_default_names_from_object,
};
use crate::batch::virtual_project::setup_props::{
    RuntimePropResolveCache,
    imports::{RuntimeImport, RuntimePropVisitSet},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_default_names_from_default_declaration<'a>(
    declaration: &'a ExportDefaultDeclarationKind<'a>,
    source: &str,
    path: &Path,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    names: &mut FxHashSet<CompactString>,
) {
    match declaration {
        ExportDefaultDeclarationKind::ObjectExpression(object) => {
            collect_default_names_from_object(
                object,
                source,
                path,
                imports,
                local_values,
                visited,
                cache,
                names,
            )
        }
        ExportDefaultDeclarationKind::CallExpression(call) => collect_default_names_from_call(
            call,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        ExportDefaultDeclarationKind::Identifier(identifier) => {
            if let Some(expr) = local_values.get(identifier.name.as_str()) {
                collect_default_names_from_expression(
                    expr,
                    source,
                    path,
                    imports,
                    local_values,
                    visited,
                    cache,
                    names,
                );
            }
        }
        ExportDefaultDeclarationKind::TSAsExpression(ts_as) => {
            collect_default_names_from_expression(
                &ts_as.expression,
                source,
                path,
                imports,
                local_values,
                visited,
                cache,
                names,
            )
        }
        ExportDefaultDeclarationKind::TSSatisfiesExpression(ts_satisfies) => {
            collect_default_names_from_expression(
                &ts_satisfies.expression,
                source,
                path,
                imports,
                local_values,
                visited,
                cache,
                names,
            )
        }
        ExportDefaultDeclarationKind::TSNonNullExpression(ts_non_null) => {
            collect_default_names_from_expression(
                &ts_non_null.expression,
                source,
                path,
                imports,
                local_values,
                visited,
                cache,
                names,
            )
        }
        ExportDefaultDeclarationKind::ParenthesizedExpression(parenthesized) => {
            collect_default_names_from_expression(
                &parenthesized.expression,
                source,
                path,
                imports,
                local_values,
                visited,
                cache,
                names,
            )
        }
        _ => {}
    }
}
