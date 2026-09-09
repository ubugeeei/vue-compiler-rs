//! Virtual TS option assembly and global-component stub collection for the
//! `check` runner.
//!
//! These helpers translate `vize.config` globals, template syntax, and project
//! `declare module "vue"` augmentations into the `VirtualTsOptions` the batch
//! type checker consumes.

#![allow(clippy::disallowed_macros)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use vize_s0::{FxHashSet, String, cstr};

use super::resolve::resolve_from_config_dir;
use crate::commands::check::tsconfig_inputs::{
    collect_tsconfig_type_packages, reference_type_packages, resolve_type_package_declaration_files,
};

mod specifier_rewrite;
mod workspace_declarations;
use specifier_rewrite::rewrite_global_component_imports_for_virtual_project;
use workspace_declarations::collect_workspace_global_component_declarations;
pub(super) use workspace_declarations::collect_workspace_global_component_declarations_for_files;

pub(super) fn build_virtual_ts_options(
    config: &crate::config::VizeConfig,
    config_dir: &Path,
) -> vize_canon::virtual_ts::VirtualTsOptions {
    let mut template_globals = config
        .global_types
        .iter()
        .map(
            |(name, declaration)| vize_canon::virtual_ts::TemplateGlobal {
                name: name.clone(),
                type_annotation: declaration.type_annotation.clone(),
                default_value: declaration.template_default_value(),
            },
        )
        .collect::<Vec<_>>();

    let globals_path = config
        .type_checker
        .globals_file
        .as_deref()
        .map(|candidate| resolve_from_config_dir(config_dir, candidate));

    if template_globals.is_empty()
        && let Some(ref globals_path) = globals_path
    {
        match parse_dts_globals(globals_path) {
            Ok(globals) => template_globals = globals,
            Err(error) => {
                eprintln!(
                    "\x1b[33mWarning:\x1b[0m Failed to parse globals from {}: {}",
                    globals_path.display(),
                    error
                );
            }
        }
    }

    vize_canon::virtual_ts::VirtualTsOptions {
        template_globals,
        ..Default::default()
    }
}

/// Resolve the configured Vue dialect for canon's virtual-TS generation.
///
/// `vue.version` is optional in `vize.config`; an unset value means the default
/// Vue 3 dialect. The resolved dialect is threaded into canon so generated
/// virtual TS can use dialect-aware instance and helper types.
pub(super) fn dialect_from_features(
    vue_version: Option<crate::config::VueVersion>,
) -> crate::config::VueVersion {
    vue_version.unwrap_or(crate::config::VueVersion::V3)
}

pub(super) fn template_syntax_mode(
    template_syntax: Option<&str>,
) -> vize_atelier_core::TemplateSyntaxMode {
    match template_syntax {
        Some("strict") => vize_atelier_core::TemplateSyntaxMode::Strict,
        Some("quirks") => vize_atelier_core::TemplateSyntaxMode::Quirks,
        Some("standard") | None => vize_atelier_core::TemplateSyntaxMode::Standard,
        Some(_) => vize_atelier_core::TemplateSyntaxMode::Standard,
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct GlobalComponentStubOptions {
    pub(super) discover_workspace_declarations: bool,
}

pub(super) fn collect_project_global_component_stubs(
    options: &mut vize_canon::virtual_ts::VirtualTsOptions,
    files: &[PathBuf],
    project_root: &Path,
    tsconfig_path: Option<&Path>,
    stub_options: GlobalComponentStubOptions,
) {
    let mut seen_names = options
        .auto_import_stubs
        .iter()
        .filter_map(|stub| declared_stub_name(stub))
        .map(String::from)
        .collect::<FxHashSet<_>>();
    let mut external_template_bindings = options
        .external_template_bindings
        .iter()
        .cloned()
        .collect::<FxHashSet<_>>();
    let mut collected = Vec::new();
    let mut package_reference_stubs = Vec::new();
    let mut seen_package_references = FxHashSet::default();
    let mut seen_declaration_paths = FxHashSet::default();

    let mut declaration_sources = Vec::new();
    for path in files.iter().filter(|path| is_declaration_path(path)) {
        push_declaration_source(
            &mut declaration_sources,
            &mut seen_declaration_paths,
            path.clone(),
            None,
        );
    }

    if stub_options.discover_workspace_declarations {
        for path in collect_workspace_global_component_declarations(project_root) {
            push_declaration_source(
                &mut declaration_sources,
                &mut seen_declaration_paths,
                path,
                None,
            );
        }
    }

    let type_package_search_start = tsconfig_path.unwrap_or(project_root);
    for package in collect_global_component_type_packages(files, tsconfig_path) {
        for path in
            resolve_type_package_declaration_files(type_package_search_start, package.as_str())
        {
            push_declaration_source(
                &mut declaration_sources,
                &mut seen_declaration_paths,
                path,
                Some(package.clone()),
            );
        }
    }

    for source in declaration_sources {
        let Ok(components) =
            super::super::dts::parse_global_component_members_with_rewritten_imports(&source.path)
        else {
            continue;
        };

        for (name, type_annotation) in components {
            let Some(name) = normalize_global_component_binding_name(name.as_str()) else {
                continue;
            };
            external_template_bindings.insert(name.clone());
            if !seen_names.insert(name.clone()) {
                continue;
            }

            if let Some(package) = source.type_package.as_deref() {
                if seen_package_references.insert(String::from(package)) {
                    package_reference_stubs.push(cstr!("/// <reference types=\"{package}\" />"));
                }
                collected.push(cstr!(
                    "declare const {name}: import(\"vue\").GlobalComponents[\"{name}\"];"
                ));
            } else {
                let type_annotation = rewrite_global_component_imports_for_virtual_project(
                    type_annotation.as_str(),
                    project_root,
                );
                collected.push(cstr!("declare const {name}: {type_annotation};"));
            }
        }
    }

    if !package_reference_stubs.is_empty() {
        let mut stubs = Vec::with_capacity(
            package_reference_stubs.len() + options.auto_import_stubs.len() + collected.len(),
        );
        stubs.extend(package_reference_stubs);
        stubs.append(&mut options.auto_import_stubs);
        stubs.extend(collected);
        options.auto_import_stubs = stubs;
    } else if !collected.is_empty() {
        options.auto_import_stubs.extend(collected);
    }
    let mut external_template_bindings = external_template_bindings.into_iter().collect::<Vec<_>>();
    external_template_bindings.sort();
    options.external_template_bindings = external_template_bindings;
}

fn push_declaration_source(
    declaration_sources: &mut Vec<GlobalComponentDeclarationSource>,
    seen_declaration_paths: &mut FxHashSet<PathBuf>,
    path: PathBuf,
    type_package: Option<String>,
) {
    if seen_declaration_paths.insert(path.clone()) {
        declaration_sources.push(GlobalComponentDeclarationSource { path, type_package });
    }
}

struct GlobalComponentDeclarationSource {
    path: PathBuf,
    type_package: Option<String>,
}

fn is_declaration_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
        })
}

fn collect_global_component_type_packages(
    files: &[PathBuf],
    tsconfig_path: Option<&Path>,
) -> Vec<String> {
    let mut packages = Vec::new();
    let mut seen = FxHashSet::default();

    for package in collect_tsconfig_type_packages(tsconfig_path) {
        push_unique_type_package(&mut packages, &mut seen, package.into());
    }

    for path in files {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for package in reference_type_packages(&content) {
            push_unique_type_package(&mut packages, &mut seen, package.into());
        }
    }

    packages
}

fn push_unique_type_package(
    packages: &mut Vec<String>,
    seen: &mut FxHashSet<String>,
    package: String,
) {
    if seen.insert(package.clone()) {
        packages.push(package);
    }
}

fn declared_stub_name(stub: &str) -> Option<&str> {
    for prefix in [
        "declare function ",
        "declare const ",
        "declare let ",
        "declare var ",
    ] {
        let Some(rest) = stub.strip_prefix(prefix) else {
            continue;
        };
        let end = rest
            .find(['<', '(', ':', '=', ';', ' '])
            .unwrap_or(rest.len());
        let name = rest[..end].trim();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn normalize_global_component_binding_name(name: &str) -> Option<String> {
    let name = name.trim().trim_matches('"').trim_matches('\'');
    if name.is_empty() {
        return None;
    }
    if name.chars().enumerate().all(|(index, ch)| {
        ch == '_'
            || ch == '$'
            || (ch.is_ascii_alphanumeric() && (index > 0 || !ch.is_ascii_digit()))
    }) {
        return Some(name.into());
    }
    None
}

/// Parse a `.d.ts` file containing `ComponentCustomProperties` augmentation.
fn parse_dts_globals(
    path: &Path,
) -> Result<Vec<vize_canon::virtual_ts::TemplateGlobal>, std::io::Error> {
    use super::super::dts::parse_interface_members;
    use vize_canon::virtual_ts::TemplateGlobal;

    Ok(
        parse_interface_members(path, "interface ComponentCustomProperties")?
            .into_iter()
            .map(|(name, type_annotation)| TemplateGlobal {
                name,
                type_annotation,
                default_value: "{} as any".into(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;
