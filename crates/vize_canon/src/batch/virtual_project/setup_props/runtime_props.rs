use std::path::Path;

use oxc_ast::ast::{BindingPattern, Declaration, Statement};
use vize_carton::{FxHashMap, FxHashSet, cstr};
use vize_croquis::macros::PropDefinition;

use super::{
    RuntimePropResolveCache,
    imports::{
        RuntimeImport, RuntimePropVisitSet, collect_imports, collect_top_level_values, parse_ts,
        resolve_runtime_import_path,
    },
};

mod collect;
mod object;
mod shape;

use collect::{collect_props_from_default_declaration, collect_props_from_expression};

pub(super) fn resolve_imported_runtime_props(
    importer: &Path,
    specifier: &str,
    imported: &str,
    root_runtime_binding: &str,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
) -> Vec<PropDefinition> {
    let Some(path) = resolve_runtime_import_path(importer, specifier) else {
        return Vec::new();
    };
    let key_path = path.canonicalize().unwrap_or_else(|_| path.clone());
    let cache_key = cstr!("{}::{}", key_path.to_string_lossy(), imported);
    if !visited.insert(cache_key.clone()) {
        return Vec::new();
    }
    if let Some(props) = cache.props(&cache_key, root_runtime_binding) {
        return props;
    }
    let Some(source) = std::fs::read_to_string(&path).ok() else {
        return Vec::new();
    };

    let props = extract_exported_runtime_props(
        &path,
        &source,
        imported,
        root_runtime_binding,
        visited,
        cache,
    );
    cache.insert_props(cache_key, props.clone());
    props
}

fn extract_exported_runtime_props(
    path: &Path,
    source: &str,
    exported_name: &str,
    root_runtime_binding: &str,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
) -> Vec<PropDefinition> {
    parse_ts(source, path, |program| {
        let imports = collect_imports(program);
        let local_values = collect_top_level_values(program);
        let mut props = Vec::new();
        for stmt in &program.body {
            match stmt {
                Statement::ExportNamedDeclaration(export_decl) => {
                    if export_decl.export_kind.is_type() {
                        continue;
                    }
                    if let Some(declaration) = export_decl.declaration.as_ref() {
                        collect_exported_declaration_props(
                            declaration,
                            exported_name,
                            source,
                            path,
                            root_runtime_binding,
                            &imports,
                            &local_values,
                            visited,
                            cache,
                            &mut props,
                        );
                    }
                    collect_exported_specifier_props(
                        export_decl,
                        exported_name,
                        source,
                        path,
                        root_runtime_binding,
                        &imports,
                        &local_values,
                        visited,
                        cache,
                        &mut props,
                    );
                }
                Statement::ExportAllDeclaration(export_decl) => {
                    if export_decl.export_kind.is_type() || export_decl.exported.is_some() {
                        continue;
                    }
                    props.extend(resolve_imported_runtime_props(
                        path,
                        export_decl.source.value.as_str(),
                        exported_name,
                        root_runtime_binding,
                        visited,
                        cache,
                    ));
                }
                Statement::ExportDefaultDeclaration(export_decl) if exported_name == "default" => {
                    collect_props_from_default_declaration(
                        &export_decl.declaration,
                        source,
                        path,
                        root_runtime_binding,
                        &imports,
                        &local_values,
                        visited,
                        cache,
                        &mut props,
                    );
                }
                _ => {}
            }
        }
        dedupe_props(props)
    })
    .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn collect_exported_specifier_props<'a>(
    export_decl: &'a oxc_ast::ast::ExportNamedDeclaration<'a>,
    exported_name: &str,
    source: &str,
    path: &Path,
    root_runtime_binding: &str,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a oxc_ast::ast::Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    props: &mut Vec<PropDefinition>,
) {
    for specifier in &export_decl.specifiers {
        if specifier.export_kind.is_type() || specifier.exported.name().as_str() != exported_name {
            continue;
        }
        let local_name = specifier.local.name().as_str();
        if let Some(reexport_source) = export_decl.source.as_ref() {
            props.extend(resolve_imported_runtime_props(
                path,
                reexport_source.value.as_str(),
                local_name,
                root_runtime_binding,
                visited,
                cache,
            ));
            continue;
        }
        if let Some(expr) = local_values.get(local_name) {
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
            continue;
        }
        if let Some(import) = imports.get(local_name) {
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
}

#[allow(clippy::too_many_arguments)]
fn collect_exported_declaration_props<'a>(
    declaration: &'a Declaration<'a>,
    exported_name: &str,
    source: &str,
    path: &Path,
    root_runtime_binding: &str,
    imports: &FxHashMap<&'a str, RuntimeImport>,
    local_values: &FxHashMap<&'a str, &'a oxc_ast::ast::Expression<'a>>,
    visited: &mut RuntimePropVisitSet,
    cache: &RuntimePropResolveCache,
    props: &mut Vec<PropDefinition>,
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
        collect_props_from_expression(
            init,
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

fn dedupe_props(props: Vec<PropDefinition>) -> Vec<PropDefinition> {
    let mut seen = FxHashSet::default();
    props
        .into_iter()
        .filter(|prop| seen.insert(prop.name.clone()))
        .collect()
}
