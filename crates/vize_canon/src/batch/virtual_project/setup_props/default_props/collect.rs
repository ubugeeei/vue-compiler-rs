use std::path::Path;

use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectExpression, ObjectPropertyKind};
use vize_carton::{CompactString, FxHashMap, FxHashSet, ToCompactString};

use super::literals::{collect_omit_key_arguments, default_value_is_undefined};
use super::resolve_imported_default_names;
use crate::batch::virtual_project::setup_props::{
    RuntimePropResolveCache,
    imports::{RuntimeImport, RuntimePropVisitSet},
    syntax::runtime_object_property_name,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_default_names_from_argument<'expr, 'ctx>(
    arg: &'expr Argument<'expr>,
    source: &str,
    path: &Path,
    imports: &FxHashMap<&'ctx str, RuntimeImport>,
    local_values: &FxHashMap<&'ctx str, &'ctx Expression<'ctx>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    names: &mut FxHashSet<CompactString>,
) {
    match arg {
        Argument::ObjectExpression(object) => collect_default_names_from_object(
            object,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Argument::Identifier(identifier) => collect_default_names_from_identifier(
            identifier.name.as_str(),
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Argument::CallExpression(call) => collect_default_names_from_call(
            call,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Argument::TSAsExpression(ts_as) => collect_default_names_from_expression(
            &ts_as.expression,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Argument::TSSatisfiesExpression(ts_satisfies) => collect_default_names_from_expression(
            &ts_satisfies.expression,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Argument::TSNonNullExpression(ts_non_null) => collect_default_names_from_expression(
            &ts_non_null.expression,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Argument::ParenthesizedExpression(parenthesized) => collect_default_names_from_expression(
            &parenthesized.expression,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_default_names_from_expression<'expr, 'ctx>(
    expr: &'expr Expression<'expr>,
    source: &str,
    path: &Path,
    imports: &FxHashMap<&'ctx str, RuntimeImport>,
    local_values: &FxHashMap<&'ctx str, &'ctx Expression<'ctx>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    names: &mut FxHashSet<CompactString>,
) {
    match expr {
        Expression::ObjectExpression(object) => collect_default_names_from_object(
            object,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Expression::Identifier(identifier) => collect_default_names_from_identifier(
            identifier.name.as_str(),
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Expression::CallExpression(call) => collect_default_names_from_call(
            call,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Expression::TSAsExpression(ts_as) => collect_default_names_from_expression(
            &ts_as.expression,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Expression::TSSatisfiesExpression(ts_satisfies) => collect_default_names_from_expression(
            &ts_satisfies.expression,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Expression::TSNonNullExpression(ts_non_null) => collect_default_names_from_expression(
            &ts_non_null.expression,
            source,
            path,
            imports,
            local_values,
            visited,
            cache,
            names,
        ),
        Expression::ParenthesizedExpression(parenthesized) => {
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

#[allow(clippy::too_many_arguments)]
fn collect_default_names_from_identifier<'ctx>(
    name: &str,
    source: &str,
    path: &Path,
    imports: &FxHashMap<&'ctx str, RuntimeImport>,
    local_values: &FxHashMap<&'ctx str, &'ctx Expression<'ctx>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    names: &mut FxHashSet<CompactString>,
) {
    if let Some(expr) = local_values.get(name) {
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
        return;
    }
    if let Some(import) = imports.get(name) {
        names.extend(resolve_imported_default_names(
            path,
            import.source.as_str(),
            import.imported.as_str(),
            visited,
            cache,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_default_names_from_call<'expr, 'ctx>(
    call: &'expr CallExpression<'expr>,
    source: &str,
    path: &Path,
    imports: &FxHashMap<&'ctx str, RuntimeImport>,
    local_values: &FxHashMap<&'ctx str, &'ctx Expression<'ctx>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    names: &mut FxHashSet<CompactString>,
) {
    match crate::batch::virtual_project::setup_props::syntax::call_expression_name(call) {
        Some("omit") => {
            let mut base = FxHashSet::default();
            if let Some(first) = call.arguments.first() {
                collect_default_names_from_argument(
                    first,
                    source,
                    path,
                    imports,
                    local_values,
                    visited,
                    cache,
                    &mut base,
                );
            }
            for omitted in collect_omit_key_arguments(call) {
                base.remove(omitted.as_str());
            }
            names.extend(base);
        }
        Some("mutable" | "reactive" | "markRaw") => {
            if let Some(first) = call.arguments.first() {
                collect_default_names_from_argument(
                    first,
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
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_default_names_from_object<'expr, 'ctx>(
    object: &'expr ObjectExpression<'expr>,
    source: &str,
    path: &Path,
    imports: &FxHashMap<&'ctx str, RuntimeImport>,
    local_values: &FxHashMap<&'ctx str, &'ctx Expression<'ctx>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    names: &mut FxHashSet<CompactString>,
) {
    for property in &object.properties {
        match property {
            ObjectPropertyKind::ObjectProperty(property) => {
                if property.computed || default_value_is_undefined(&property.value) {
                    continue;
                }
                let Some(name) = runtime_object_property_name(&property.key) else {
                    continue;
                };
                names.insert(name.to_compact_string());
            }
            ObjectPropertyKind::SpreadProperty(spread) => collect_default_names_from_expression(
                &spread.argument,
                source,
                path,
                imports,
                local_values,
                visited,
                cache,
                names,
            ),
        }
    }
}
