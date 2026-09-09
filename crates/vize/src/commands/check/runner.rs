//! Check command execution logic.
//! The direct runner materializes one Corsa program per owning tsconfig so
//! project references retain their own compiler options.

#![allow(clippy::disallowed_macros)]

use std::{
    path::Path,
    time::{Duration, Instant},
};

use super::{
    CheckArgs,
    imports::ImportFileOptions,
    imports_package_routes::sort_package_route_bindings,
    path_cache::CanonicalPathCache,
    patterns::CheckFileOptions,
    reporting::{JsonFileResult, JsonOutput, JsonProgramResult},
    tsconfig_inputs::{TsconfigInputCache, tsconfig_allows_js},
};

mod collect;
mod default_imports;
mod diagnostics;
mod direct;
mod execution;
mod global_components;
mod ignores;
mod input_scope;
mod invocation;
mod nuxt_tsconfig;
mod output;
mod program_inputs;
mod programs;
mod resolve;
#[cfg(unix)]
mod socket;
#[cfg(test)]
mod tests;
mod text_style;

use collect::collect_check_files_with_ignores;
use default_imports::{
    DefaultRunFileContext, ExplicitAmbientImportContext, LocalImportContext, canonical_file_set,
    collect_default_run_files, register_ambient_declaration_files,
    register_explicit_ambient_imports, register_transitive_local_imports,
};
#[cfg(test)]
use diagnostics::is_suppressed_false_positive;
pub(crate) use direct::run_direct;
use execution::{CheckerSettings, ProgramExecution, ProgramExecutionInput, execute_program};
use global_components::{
    GlobalComponentStubOptions, build_virtual_ts_options, collect_project_global_component_stubs,
    collect_workspace_global_component_declarations_for_files, dialect_from_features,
    template_syntax_mode,
};
use ignores::load_check_ignore_set;
use input_scope::{exit_if_default_run_leaves_cwd, report_no_inputs};
use invocation::{resolve_invocation_program, resolve_nuxt_project_root};
use nuxt_tsconfig::resolve_checker_tsconfig_path;
use output::{exit_after_execution_error, finish_executions};
use program_inputs::{ProgramInputContext, filter_for_program, project_graph_allows_js};
use programs::{CollectedRoots, ProgramCandidate, split_program_candidates};
use resolve::{
    display_path, explicit_input_root, resolve_declaration_emit_options, resolve_from_config_dir,
    resolve_project_root, resolve_tsconfig_path, validate_corsa_server_count,
    validate_inputs_in_root,
};
#[cfg(test)]
use resolve::{find_nearest_tsconfig_dir, resolve_declaration_dir};
#[cfg(unix)]
pub(crate) use socket::run_with_socket;
use text_style::TextStyle;

#[allow(clippy::too_many_arguments)]
fn collect_roots(
    args: &CheckArgs,
    invocation_project_root: &Path,
    cwd: &Path,
    invocation_tsconfig_path: Option<&Path>,
    jsx_typecheck: bool,
    cache: &mut TsconfigInputCache,
    canonical_paths: &mut CanonicalPathCache,
    check_ignore_set: Option<&ignores::CheckIgnoreSet>,
    package_routes: &mut vize_canon::PackageRouteResolver,
) -> CollectedRoots {
    let include_js = project_graph_allows_js(invocation_tsconfig_path, cache);
    let import_options = ImportFileOptions {
        include_js,
        include_jsx: jsx_typecheck,
    };
    if args.patterns.is_empty() {
        let collected = collect_default_run_files(
            DefaultRunFileContext {
                project_root: invocation_project_root,
                cwd,
                tsconfig_path: invocation_tsconfig_path,
                import_options,
                check_ignore_set,
            },
            cache,
            canonical_paths,
            package_routes,
        );
        exit_if_default_run_leaves_cwd(
            &collected.files,
            cwd,
            invocation_project_root,
            invocation_tsconfig_path,
            args.quiet,
        );
        return collected;
    }

    let files = collect_check_files_with_ignores(
        &args.patterns,
        CheckFileOptions {
            include_js,
            include_jsx: jsx_typecheck,
        },
        check_ignore_set,
    );
    let reported = canonical_file_set(&files, canonical_paths);
    CollectedRoots {
        files: files.clone(),
        inputs: files,
        reported,
        package_routes: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_and_execute(
    args: &CheckArgs,
    mut candidate: ProgramCandidate,
    cwd: &Path,
    invocation_project_root: &Path,
    nuxt_project_root: &Path,
    explicit_input_root: &Path,
    validate_inputs: bool,
    jsx_typecheck: bool,
    settings: &CheckerSettings,
    cache: &mut TsconfigInputCache,
    canonical_paths: &mut CanonicalPathCache,
    package_route_resolver: &mut vize_canon::PackageRouteResolver,
) -> Result<Option<ProgramExecution>, vize_s0::String> {
    let initial_root = candidate
        .tsconfig_path
        .as_deref()
        .and_then(Path::parent)
        .unwrap_or(invocation_project_root);
    let logical_program_root = if candidate.rebuild_supporting_files {
        Some(initial_root.to_path_buf())
    } else if args.patterns.is_empty() {
        Some(invocation_project_root.to_path_buf())
    } else {
        None
    };
    let import_options = ImportFileOptions {
        include_js: candidate
            .tsconfig_path
            .as_deref()
            .is_some_and(|path| tsconfig_allows_js(path, cache)),
        include_jsx: jsx_typecheck,
    };
    let mut import_time = Duration::ZERO;
    let mut authored_imports = Vec::new();
    let mut package_routes = std::mem::take(&mut candidate.package_routes);
    if !args.patterns.is_empty() || candidate.rebuild_supporting_files {
        let import_start = Instant::now();
        let discovered = register_transitive_local_imports(
            &mut candidate.files,
            LocalImportContext {
                cwd,
                tsconfig_path: candidate.tsconfig_path.as_deref(),
                import_options,
                explicit_input_root: Some(explicit_input_root),
                validate_inputs,
            },
            canonical_paths,
            package_route_resolver,
        );
        import_time += import_start.elapsed();
        authored_imports = discovered.authored;
        package_routes.extend(discovered.package_routes);
    }
    if args.patterns.is_empty() && candidate.rebuild_supporting_files {
        register_ambient_declaration_files(
            &mut candidate.files,
            initial_root,
            candidate.tsconfig_path.as_deref(),
            cache,
        );
        let import_start = Instant::now();
        let discovered = register_transitive_local_imports(
            &mut candidate.files,
            LocalImportContext {
                cwd,
                tsconfig_path: candidate.tsconfig_path.as_deref(),
                import_options,
                explicit_input_root: Some(explicit_input_root),
                validate_inputs,
            },
            canonical_paths,
            package_route_resolver,
        );
        import_time += import_start.elapsed();
        package_routes.extend(discovered.package_routes);
    }
    validate_inputs_in_root(explicit_input_root, &candidate.files, validate_inputs)?;

    let project_root =
        resolve_project_root(candidate.tsconfig_path.as_deref(), cwd, &candidate.files);
    let discovered_tsconfig_path = candidate
        .tsconfig_path
        .clone()
        .or_else(|| resolve_tsconfig_path(None, cwd, &project_root, &candidate.files));
    let program_tsconfig_path = filter_for_program(ProgramInputContext {
        tsconfig_path: discovered_tsconfig_path.as_deref(),
        explicit: !args.patterns.is_empty(),
        include_jsx: jsx_typecheck,
        files: &mut candidate.files,
        inputs: &mut candidate.inputs,
        reported: &mut candidate.reported,
        cache,
        canonical_paths,
    });
    if !args.patterns.is_empty() && candidate.inputs.is_empty() {
        return Ok(None);
    }
    candidate.reported.extend(
        authored_imports
            .into_iter()
            .map(|path| canonical_paths.canonicalize(&path)),
    );
    if !args.patterns.is_empty()
        && let Some(program_tsconfig_path) = program_tsconfig_path.as_deref()
    {
        let workspace_global_component_declarations =
            collect_workspace_global_component_declarations_for_files(
                &project_root,
                &candidate.files,
            );
        let import_start = Instant::now();
        package_routes.extend(register_explicit_ambient_imports(
            &mut candidate.files,
            ExplicitAmbientImportContext::new(
                &project_root,
                cwd,
                program_tsconfig_path,
                explicit_input_root,
                &workspace_global_component_declarations,
                import_options,
            ),
            cache,
            canonical_paths,
            package_route_resolver,
        ));
        import_time += import_start.elapsed();
    }
    let project_root =
        resolve_project_root(program_tsconfig_path.as_deref(), cwd, &candidate.files);
    let logical_program_root = logical_program_root.unwrap_or_else(|| project_root.clone());
    resolve::retain_project_files(&mut candidate.files, &project_root);
    if candidate.files.is_empty() {
        return Ok(None);
    }

    sort_package_route_bindings(&mut package_routes);
    let discover_global_component_declarations =
        !args.patterns.is_empty() && program_tsconfig_path.is_none();
    let mut execution = execute_program(
        ProgramExecutionInput {
            files: &candidate.files,
            reported_files: candidate.reported,
            package_routes: &package_routes,
            project_root: &project_root,
            program_root: logical_program_root,
            tsconfig_path: program_tsconfig_path,
            nuxt_project_root,
            package_route_resolver: package_route_resolver.clone(),
            discover_global_component_declarations,
        },
        settings,
    )?;
    execution.import_time = import_time;
    Ok(Some(execution))
}

fn validate_config_arg(args: &CheckArgs) {
    if let Some(path) = args.config.as_deref()
        && !args.no_config
        && let Err(error) = crate::config::validate_explicit_config_path(path)
    {
        let style = TextStyle::stderr();
        eprintln!("{} {}", style.red("Error:"), error);
        std::process::exit(2);
    }
}

#[cfg(not(feature = "legacy"))]
fn warn_for_disabled_legacy(requested: bool) {
    if requested {
        let style = TextStyle::stderr();
        eprintln!(
            "{} a Vue 2 dialect is configured (`typeChecker.legacyVue2` \
             or `vue.version` 2/2.7) but this `vize` build has no legacy Vue support; rebuild \
             with `--features legacy` to enable Vue 2 Options API type checking.",
            style.yellow("warning:")
        );
    }
}

#[cfg(feature = "legacy")]
fn warn_for_disabled_legacy(_requested: bool) {}
