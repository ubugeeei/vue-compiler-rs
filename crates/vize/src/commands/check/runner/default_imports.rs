use std::{
    fs,
    path::{Path, PathBuf},
};

use vize_s0::FxHashSet;

use super::{
    CollectedRoots,
    collect::path_is_inside_root,
    ignores::{CheckIgnoreSet, retain_unignored},
};
use crate::commands::check::{
    imports::{
        ImportFileOptions, TransitiveLocalImports, collect_transitive_local_imports_with_resolver,
    },
    imports_aliases::PathAliasResolver,
    imports_package_routes::sort_package_route_bindings,
    path_cache::CanonicalPathCache,
    tsconfig_inputs::{
        TsconfigInputCache, collect_ambient_declaration_files, collect_default_check_files,
        collect_hidden_ambient_declaration_files,
    },
};

pub(super) struct ExplicitAmbientImportContext<'a> {
    project_root: &'a Path,
    cwd: &'a Path,
    tsconfig_path: &'a Path,
    explicit_input_root: &'a Path,
    additional_ambient_declarations: &'a [PathBuf],
    import_options: ImportFileOptions,
}

pub(super) struct RegisteredLocalImports {
    pub(super) authored: Vec<PathBuf>,
    pub(super) package_routes: Vec<vize_canon::PackageRouteBinding>,
}

#[derive(Clone, Copy)]
pub(super) struct DefaultRunFileContext<'a> {
    pub(super) project_root: &'a Path,
    pub(super) cwd: &'a Path,
    pub(super) tsconfig_path: Option<&'a Path>,
    pub(super) import_options: ImportFileOptions,
    pub(super) check_ignore_set: Option<&'a CheckIgnoreSet>,
}

#[derive(Clone, Copy)]
pub(super) struct LocalImportContext<'a> {
    pub(super) cwd: &'a Path,
    pub(super) tsconfig_path: Option<&'a Path>,
    pub(super) import_options: ImportFileOptions,
    pub(super) explicit_input_root: Option<&'a Path>,
    pub(super) validate_inputs: bool,
}

impl<'a> ExplicitAmbientImportContext<'a> {
    pub(super) fn new(
        project_root: &'a Path,
        cwd: &'a Path,
        tsconfig_path: &'a Path,
        explicit_input_root: &'a Path,
        additional_ambient_declarations: &'a [PathBuf],
        import_options: ImportFileOptions,
    ) -> Self {
        Self {
            project_root,
            cwd,
            tsconfig_path,
            explicit_input_root,
            additional_ambient_declarations,
            import_options,
        }
    }
}

pub(super) fn collect_default_run_files(
    context: DefaultRunFileContext<'_>,
    tsconfig_input_cache: &mut TsconfigInputCache,
    canonical_paths: &mut CanonicalPathCache,
    resolver: &mut vize_canon::PackageRouteResolver,
) -> CollectedRoots {
    let mut files = collect_default_check_files(
        context.project_root,
        context.tsconfig_path,
        context.import_options.include_jsx,
        tsconfig_input_cache,
    );
    retain_unignored(&mut files, context.check_ignore_set);
    let inputs = files.clone();
    let mut reported_files = canonical_file_set(&files, canonical_paths);
    let discovered = register_transitive_local_imports(
        &mut files,
        LocalImportContext {
            cwd: context.cwd,
            tsconfig_path: context.tsconfig_path,
            import_options: context.import_options,
            explicit_input_root: None,
            validate_inputs: false,
        },
        canonical_paths,
        resolver,
    );
    reported_files.extend(canonical_file_set(&discovered.authored, canonical_paths));
    let mut package_routes = discovered.package_routes;
    register_ambient_declaration_files(
        &mut files,
        context.project_root,
        context.tsconfig_path,
        tsconfig_input_cache,
    );
    // Imports reached only through hidden ambient declarations provide type
    // context, but are not authored members of the checked program.
    let hidden_discovered = register_transitive_local_imports(
        &mut files,
        LocalImportContext {
            cwd: context.cwd,
            tsconfig_path: context.tsconfig_path,
            import_options: context.import_options,
            explicit_input_root: None,
            validate_inputs: false,
        },
        canonical_paths,
        resolver,
    );
    package_routes.extend(hidden_discovered.package_routes);
    sort_package_route_bindings(&mut package_routes);

    CollectedRoots {
        files,
        inputs,
        reported: reported_files,
        package_routes,
    }
}

pub(super) fn register_ambient_declaration_files(
    files: &mut Vec<PathBuf>,
    project_root: &Path,
    tsconfig_path: Option<&Path>,
    tsconfig_input_cache: &mut TsconfigInputCache,
) {
    for path in
        collect_hidden_ambient_declaration_files(project_root, tsconfig_path, tsconfig_input_cache)
    {
        if !files.contains(&path) {
            files.push(path);
        }
    }
}

pub(super) fn register_explicit_ambient_imports(
    files: &mut Vec<PathBuf>,
    context: ExplicitAmbientImportContext<'_>,
    tsconfig_input_cache: &mut TsconfigInputCache,
    canonical_paths: &mut CanonicalPathCache,
    package_routes: &mut vize_canon::PackageRouteResolver,
) -> Vec<vize_canon::PackageRouteBinding> {
    let keep_package_local =
        super::resolve::project_root_has_package_boundary(context.project_root);
    let mut ambient_declarations = collect_ambient_declaration_files(
        context.project_root,
        Some(context.tsconfig_path),
        tsconfig_input_cache,
    )
    .into_iter()
    .filter(|path| !keep_package_local || path.starts_with(context.project_root))
    .collect::<Vec<_>>();
    ambient_declarations.extend(
        context
            .additional_ambient_declarations
            .iter()
            .filter(|path| !keep_package_local || path.starts_with(context.project_root))
            .cloned(),
    );
    ambient_declarations.sort();
    ambient_declarations.dedup();
    let program_ambient_declarations = ambient_declarations
        .iter()
        .filter(|path| should_register_explicit_ambient_declaration(path))
        .cloned()
        .collect::<Vec<_>>();
    let discovered = collect_transitive_local_imports_from(
        &ambient_declarations,
        LocalImportContext {
            cwd: context.cwd,
            tsconfig_path: Some(context.tsconfig_path),
            import_options: context.import_options,
            explicit_input_root: Some(context.explicit_input_root),
            validate_inputs: true,
        },
        canonical_paths,
        package_routes,
    );
    files.extend(discovered.registrations);
    files.extend(program_ambient_declarations);
    files.sort();
    files.dedup();
    discovered.package_routes
}

fn should_register_explicit_ambient_declaration(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return true;
    };
    !is_non_module_vue_ambient_declaration(&content)
}

fn is_non_module_vue_ambient_declaration(content: &str) -> bool {
    contains_vue_module_declaration(content) && !has_external_module_marker(content)
}

fn contains_vue_module_declaration(content: &str) -> bool {
    content.contains("declare module \"vue\"") || content.contains("declare module 'vue'")
}

fn has_external_module_marker(content: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("import ")
            || line.starts_with("import\"")
            || line.starts_with("import'")
            || line.starts_with("export {}")
            || line.starts_with("export{}")
    })
}

pub(super) fn canonical_file_set(
    files: &[PathBuf],
    canonical_paths: &mut CanonicalPathCache,
) -> FxHashSet<PathBuf> {
    files
        .iter()
        .map(|path| canonical_paths.canonicalize(path))
        .collect()
}

pub(super) fn register_transitive_local_imports(
    files: &mut Vec<PathBuf>,
    context: LocalImportContext<'_>,
    canonical_paths: &mut CanonicalPathCache,
    package_routes: &mut vize_canon::PackageRouteResolver,
) -> RegisteredLocalImports {
    let discovered = collect_local_imports(
        files,
        context.cwd,
        context.tsconfig_path,
        context.import_options,
        canonical_paths,
        package_routes,
    );
    // The explicit-root boundary constrains user-selected roots and files that
    // enter Vize's mirror. It must not hide authored modules that TypeScript
    // legitimately resolves in place outside that boundary.
    let TransitiveLocalImports {
        registrations,
        authored,
        package_routes,
    } = discovered;
    append_local_imports(
        files,
        registrations,
        context.explicit_input_root,
        context.validate_inputs,
    );
    RegisteredLocalImports {
        authored,
        package_routes,
    }
}

pub(super) fn collect_transitive_local_imports_from(
    roots: &[PathBuf],
    context: LocalImportContext<'_>,
    canonical_paths: &mut CanonicalPathCache,
    package_routes: &mut vize_canon::PackageRouteResolver,
) -> TransitiveLocalImports {
    let mut discovered = collect_local_imports(
        roots,
        context.cwd,
        context.tsconfig_path,
        context.import_options,
        canonical_paths,
        package_routes,
    );
    discovered.registrations.retain(|path| {
        local_import_is_allowed(path, context.explicit_input_root, context.validate_inputs)
    });
    discovered
}

fn collect_local_imports(
    roots: &[PathBuf],
    cwd: &Path,
    tsconfig_path: Option<&Path>,
    import_options: ImportFileOptions,
    canonical_paths: &mut CanonicalPathCache,
    package_routes: &mut vize_canon::PackageRouteResolver,
) -> TransitiveLocalImports {
    let aliases = PathAliasResolver::from_tsconfig(tsconfig_path);
    collect_transitive_local_imports_with_resolver(
        roots,
        cwd,
        canonical_paths,
        import_options,
        Some(&aliases),
        package_routes,
    )
}

fn append_local_imports(
    files: &mut Vec<PathBuf>,
    discovered: Vec<PathBuf>,
    explicit_input_root: Option<&Path>,
    validate_inputs: bool,
) -> Vec<PathBuf> {
    let mut appended = Vec::new();
    let mut known: FxHashSet<PathBuf> = files.iter().cloned().collect();
    for path in discovered {
        if local_import_is_allowed(&path, explicit_input_root, validate_inputs)
            && known.insert(path.clone())
        {
            files.push(path.clone());
            appended.push(path);
        }
    }
    files.sort();
    files.dedup();
    appended
}

fn local_import_is_allowed(
    path: &Path,
    explicit_input_root: Option<&Path>,
    validate_inputs: bool,
) -> bool {
    !validate_inputs || explicit_input_root.is_none_or(|root| path_is_inside_root(root, path))
}

#[cfg(test)]
#[path = "default_imports_tests.rs"]
mod tests;
