use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, Declaration, Expression, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;
use vize_carton::{CompactString, FxHashMap, FxHashSet, ToCompactString, cstr};
use vize_croquis::{Croquis, macros::MacroKind};

use super::{
    RuntimePropResolveCache,
    imports::{
        RuntimeImport, RuntimePropVisitSet, collect_imports, collect_top_level_values, parse_ts,
        resolve_runtime_import_path,
    },
    syntax::call_expression_name,
};

mod collect;
mod declaration;
mod literals;

use collect::{collect_default_names_from_argument, collect_default_names_from_expression};
use declaration::collect_default_names_from_default_declaration;

pub(super) fn merge_type_based_with_defaults_into_croquis(
    croquis: &mut Croquis,
    script_setup_source: &str,
    script_source: Option<&str>,
    path: &Path,
    cache: &RuntimePropResolveCache,
) {
    let Some(define_props) = croquis.macros.define_props() else {
        return;
    };
    if define_props.type_args.is_none() {
        return;
    }

    let mut default_names = FxHashSet::default();
    for call in croquis
        .macros
        .all_calls()
        .iter()
        .filter(|call| call.kind == MacroKind::WithDefaults)
    {
        let Some(runtime_args) = call.runtime_args.as_deref() else {
            continue;
        };
        collect_type_based_with_defaults_default_names(
            runtime_args,
            script_setup_source,
            script_source,
            path,
            &mut default_names,
            cache,
        );
    }

    for name in default_names {
        croquis
            .macros
            .mark_prop_default_value(name.as_str(), "undefined".to_compact_string());
    }
}

fn collect_type_based_with_defaults_default_names(
    runtime_args: &str,
    script_setup_source: &str,
    script_source: Option<&str>,
    path: &Path,
    names: &mut FxHashSet<CompactString>,
    cache: &RuntimePropResolveCache,
) {
    let expression_source = cstr!("withDefaults({runtime_args})");
    let mut visited = FxHashSet::default();
    let allocator = Allocator::default();
    let Ok(Expression::CallExpression(call)) =
        Parser::new(&allocator, expression_source.as_str(), SourceType::ts()).parse_expression()
    else {
        return;
    };
    if call_expression_name(&call) != Some("withDefaults") {
        return;
    }
    let Some(defaults) = call.arguments.get(1) else {
        return;
    };

    collect_default_names_with_script_context(
        defaults,
        script_setup_source,
        path,
        &mut visited,
        names,
        cache,
    );

    if let Some(script_source) = script_source {
        collect_default_names_with_script_context(
            defaults,
            script_source,
            path,
            &mut visited,
            names,
            cache,
        );
    }
}

fn collect_default_names_with_script_context(
    defaults: &oxc_ast::ast::Argument<'_>,
    context_source: &str,
    path: &Path,
    visited: &mut RuntimePropVisitSet,
    names: &mut FxHashSet<CompactString>,
    cache: &RuntimePropResolveCache,
) {
    parse_ts(context_source, "script.ts", |program| {
        let imports = collect_imports(program);
        let local_values = collect_top_level_values(program);
        collect_default_names_from_argument(
            defaults,
            context_source,
            path,
            &imports,
            &local_values,
            visited,
            cache,
            names,
        );
    });
}

pub(super) fn resolve_imported_default_names(
    importer: &Path,
    specifier: &str,
    imported: &str,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
) -> FxHashSet<CompactString> {
    let Some(path) = resolve_runtime_import_path(importer, specifier) else {
        return FxHashSet::default();
    };
    let key_path = path.canonicalize().unwrap_or_else(|_| path.clone());
    let key = cstr!("{}::{}", key_path.to_string_lossy(), imported);
    if !visited.insert(key.clone()) {
        return FxHashSet::default();
    }
    if let Some(names) = cache.default_names(&key) {
        return names;
    }
    let Some(source) = std::fs::read_to_string(&path).ok() else {
        return FxHashSet::default();
    };

    let names = extract_exported_default_names(&path, &source, imported, visited, cache);
    cache.insert_default_names(key, names.clone());
    names
}

fn extract_exported_default_names(
    path: &Path,
    source: &str,
    exported_name: &str,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
) -> FxHashSet<CompactString> {
    parse_ts(source, path, |program| {
        let imports = collect_imports(program);
        let local_values = collect_top_level_values(program);
        let mut names = FxHashSet::default();
        for stmt in &program.body {
            match stmt {
                Statement::ExportNamedDeclaration(export_decl) => {
                    if export_decl.export_kind.is_type() {
                        continue;
                    }
                    if let Some(declaration) = export_decl.declaration.as_ref() {
                        collect_exported_declaration_default_names(
                            declaration,
                            exported_name,
                            source,
                            path,
                            &imports,
                            &local_values,
                            visited,
                            cache,
                            &mut names,
                        );
                    }
                    collect_exported_specifier_default_names(
                        export_decl,
                        exported_name,
                        source,
                        path,
                        &imports,
                        &local_values,
                        visited,
                        cache,
                        &mut names,
                    );
                }
                Statement::ExportAllDeclaration(export_decl) => {
                    if export_decl.export_kind.is_type() || export_decl.exported.is_some() {
                        continue;
                    }
                    names.extend(resolve_imported_default_names(
                        path,
                        export_decl.source.value.as_str(),
                        exported_name,
                        visited,
                        cache,
                    ));
                }
                Statement::ExportDefaultDeclaration(export_decl) if exported_name == "default" => {
                    collect_default_names_from_default_declaration(
                        &export_decl.declaration,
                        source,
                        path,
                        &imports,
                        &local_values,
                        visited,
                        cache,
                        &mut names,
                    );
                }
                _ => {}
            }
        }
        names
    })
    .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn collect_exported_specifier_default_names<'a>(
    export_decl: &'a oxc_ast::ast::ExportNamedDeclaration<'a>,
    exported_name: &str,
    source: &str,
    path: &Path,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    names: &mut FxHashSet<CompactString>,
) {
    for specifier in &export_decl.specifiers {
        if specifier.export_kind.is_type() || specifier.exported.name().as_str() != exported_name {
            continue;
        }
        let local_name = specifier.local.name().as_str();
        if let Some(reexport_source) = export_decl.source.as_ref() {
            names.extend(resolve_imported_default_names(
                path,
                reexport_source.value.as_str(),
                local_name,
                visited,
                cache,
            ));
            continue;
        }
        if let Some(expr) = local_values.get(local_name) {
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
            continue;
        }
        if let Some(import) = imports.get(local_name) {
            names.extend(resolve_imported_default_names(
                path,
                import.source.as_str(),
                import.imported.as_str(),
                visited,
                cache,
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_exported_declaration_default_names<'a>(
    declaration: &'a Declaration<'a>,
    exported_name: &str,
    source: &str,
    path: &Path,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    names: &mut FxHashSet<CompactString>,
) {
    let Declaration::VariableDeclaration(variable) = declaration else {
        return;
    };
    for declarator in &variable.declarations {
        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            continue;
        };
        if id.name.as_str() != exported_name {
            continue;
        }
        let Some(init) = declarator.init.as_ref() else {
            continue;
        };
        collect_default_names_from_expression(
            init,
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
