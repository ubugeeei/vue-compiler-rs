use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use ignore::{DirEntry, WalkBuilder};
use vize_croquis::{Analyzer, AnalyzerOptions, naming::to_pascal_case};
use vize_s0::{FxHashSet, String};

use super::{is_declaration_path, normalize_global_component_binding_name};

pub(super) fn collect_workspace_global_component_declarations(project_root: &Path) -> Vec<PathBuf> {
    collect_workspace_global_component_declarations_inner(project_root, None)
}

pub(in crate::commands::check::runner) fn collect_workspace_global_component_declarations_for_files(
    project_root: &Path,
    files: &[PathBuf],
) -> Vec<PathBuf> {
    let component_names = collect_explicit_template_component_names(files);
    collect_workspace_global_component_declarations_inner(project_root, Some(&component_names))
}

fn collect_workspace_global_component_declarations_inner(
    project_root: &Path,
    component_names: Option<&FxHashSet<String>>,
) -> Vec<PathBuf> {
    let mut builder = WalkBuilder::new(project_root);
    builder
        .hidden(false)
        .follow_links(false)
        .filter_entry(should_visit_global_component_declaration_entry);

    let mut paths = Vec::new();
    for entry in builder.build().flatten() {
        let path = entry.path();
        if !entry.file_type().is_some_and(|kind| kind.is_file()) || !is_declaration_path(path) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() <= 4 * 1024 * 1024
            && declaration_may_augment_global_components(path, component_names)
        {
            paths.push(path.to_path_buf());
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn collect_explicit_template_component_names(files: &[PathBuf]) -> FxHashSet<String> {
    let mut names = FxHashSet::default();
    for path in files {
        if !is_vue_path(path) {
            continue;
        }
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(descriptor) = vize_atelier_sfc::parse_sfc(
            &source,
            vize_atelier_sfc::SfcParseOptions {
                filename: path.to_string_lossy().into_owned().into(),
                ..Default::default()
            },
        ) else {
            continue;
        };
        let Some(template) = descriptor.template.as_ref() else {
            continue;
        };
        let allocator = vize_s0::Allocator::new();
        let (root, _) = vize_armature::parse(&allocator, template.content.as_ref());
        let mut analyzer = Analyzer::with_options(AnalyzerOptions::full());
        analyzer.analyze_template(&root);
        for usage in analyzer.finish().component_usages {
            names.insert(String::from(usage.name.as_str()));
            names.insert(String::from(to_pascal_case(usage.name.as_str()).as_str()));
        }
    }
    names
}

fn should_visit_global_component_declaration_entry(entry: &DirEntry) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_some_and(|kind| kind.is_dir()) {
        return true;
    }
    !is_excluded_global_component_declaration_dir(entry.file_name())
}

fn is_excluded_global_component_declaration_dir(name: &OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(
            ".git"
                | ".vize"
                | ".vize-baseline"
                | ".yarn"
                | "coverage"
                | "dist"
                | "node_modules"
                | "target"
        )
    )
}

fn declaration_may_augment_global_components(
    path: &Path,
    component_names: Option<&FxHashSet<String>>,
) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    if !content.contains("GlobalComponents") {
        return false;
    }
    let Some(component_names) = component_names else {
        return true;
    };
    if component_names.is_empty() {
        return false;
    }
    super::super::super::dts_ast::parse_global_component_members_content(&content)
        .into_iter()
        .any(|(name, _)| {
            normalize_global_component_binding_name(name.as_str())
                .is_some_and(|name| component_names.contains(&name))
        })
}

fn is_vue_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("vue"))
}
