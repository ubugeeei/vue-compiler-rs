//! One compiler-options-homogeneous Corsa program within a check invocation.

use std::{path::PathBuf, time::Duration, time::Instant};

use vize_canon::{
    BatchTypeCheckResult, BatchTypeChecker, BatchTypeCheckerOptions,
    batch::TypeChecker as BatchTypeCheckerTrait,
};
use vize_s0::{FxHashSet, String, config::VueVersion, cstr};

use super::nuxt_tsconfig::{
    PreparedCheckerTsconfig, wait_for_active_config_test_barrier,
    wait_for_prepared_config_test_barrier,
};
use super::{
    GlobalComponentStubOptions, collect_project_global_component_stubs,
    resolve_checker_tsconfig_path,
};
use crate::commands::check::nuxt;

#[derive(Clone)]
pub(super) struct CheckerSettings {
    pub(super) virtual_ts_options: vize_canon::virtual_ts::VirtualTsOptions,
    pub(super) corsa_path: Option<PathBuf>,
    pub(super) servers: Option<usize>,
    pub(super) options_api: bool,
    pub(super) legacy_vue2: bool,
    pub(super) jsx_typecheck: bool,
    pub(super) template_syntax: vize_atelier_core::TemplateSyntaxMode,
    pub(super) experimental_in_tag_comments: bool,
    pub(super) dialect: VueVersion,
    pub(super) check_props: bool,
    pub(super) check_template_bindings: bool,
    pub(super) check_emits: bool,
    pub(super) quiet: bool,
}

pub(super) struct ProgramExecution {
    pub(super) checker: BatchTypeChecker,
    pub(super) result: BatchTypeCheckResult,
    pub(super) input_files: Vec<PathBuf>,
    pub(super) reported_files: FxHashSet<PathBuf>,
    pub(super) tsconfig_path: Option<PathBuf>,
    pub(super) program_root: PathBuf,
    pub(super) import_time: Duration,
    pub(super) gen_time: Duration,
    pub(super) check_time: Duration,
    _checker_tsconfig: PreparedCheckerTsconfig,
}

pub(super) struct ProgramExecutionInput<'a> {
    pub(super) files: &'a [PathBuf],
    pub(super) reported_files: FxHashSet<PathBuf>,
    pub(super) package_routes: &'a [vize_canon::PackageRouteBinding],
    pub(super) project_root: &'a std::path::Path,
    pub(super) program_root: PathBuf,
    pub(super) tsconfig_path: Option<PathBuf>,
    pub(super) nuxt_project_root: &'a std::path::Path,
    pub(super) package_route_resolver: vize_canon::PackageRouteResolver,
    pub(super) discover_global_component_declarations: bool,
}

pub(super) fn execute_program(
    input: ProgramExecutionInput<'_>,
    settings: &CheckerSettings,
) -> Result<ProgramExecution, String> {
    let mut virtual_ts_options = settings.virtual_ts_options.clone();
    let nuxt_path_aliases = nuxt::detect(
        &mut virtual_ts_options,
        input.nuxt_project_root,
        input.tsconfig_path.as_deref(),
        settings.legacy_vue2,
        settings.dialect,
    );
    collect_project_global_component_stubs(
        &mut virtual_ts_options,
        input.files,
        input.project_root,
        input.tsconfig_path.as_deref(),
        GlobalComponentStubOptions {
            discover_workspace_declarations: input.discover_global_component_declarations,
        },
    );
    let checker_tsconfig = resolve_checker_tsconfig_path(
        input.tsconfig_path.as_deref(),
        input.project_root,
        input.nuxt_project_root,
        &nuxt_path_aliases,
    )
    .map_err(|error| cstr!("Failed to prepare type checker tsconfig: {}", error))?;
    wait_for_prepared_config_test_barrier(&checker_tsconfig).map_err(|error| {
        cstr!(
            "Failed to synchronize prepared type checker config: {}",
            error
        )
    })?;
    if !settings.quiet {
        eprintln!(
            "Building Corsa virtual project for {} files under {}...",
            input.files.len(),
            input.program_root.display()
        );
    }

    let gen_start = Instant::now();
    let mut checker = BatchTypeChecker::with_options_and_corsa_path(
        input.project_root,
        BatchTypeCheckerOptions {
            tsconfig_path: checker_tsconfig.path().map(PathBuf::from),
            virtual_ts_options,
        },
        settings.corsa_path.as_deref(),
    )
    .map_err(|error| cstr!("{}", error))?;
    checker.set_server_count(settings.servers);
    if settings.options_api {
        checker.enable_options_api();
    }
    #[cfg(feature = "legacy")]
    if settings.legacy_vue2 {
        checker.enable_legacy_vue2();
    }
    if settings.jsx_typecheck {
        checker.enable_jsx_typecheck();
    }
    checker.set_template_syntax(settings.template_syntax);
    checker.set_experimental_in_tag_comments(settings.experimental_in_tag_comments);
    checker.set_dialect(settings.dialect);
    checker.set_virtual_ts_checks(
        settings.check_props,
        settings.check_template_bindings,
        settings.check_emits,
    );
    checker.set_package_route_resolver(input.package_route_resolver);
    checker.set_package_routes(input.package_routes.iter().cloned());
    checker
        .scan_paths(input.files)
        .map_err(|error| cstr!("{}", error))?;
    checker.set_diagnostic_paths(input.reported_files.iter().map(PathBuf::as_path));
    let gen_time = gen_start.elapsed();

    if !settings.quiet {
        eprintln!(
            "Running Corsa diagnostics for {} files...",
            checker.virtual_files().len()
        );
    }
    let check_start = Instant::now();
    let mut result = checker
        .check_project()
        .map_err(|error| cstr!("{}", error))?;
    if wait_for_active_config_test_barrier(&checker_tsconfig).map_err(|error| {
        cstr!(
            "Failed to synchronize active type checker config: {}",
            error
        )
    })? {
        result = checker
            .check_project()
            .map_err(|error| cstr!("{}", error))?;
    }
    if let (Some(authored), Some(prepared)) =
        (input.tsconfig_path.as_ref(), checker_tsconfig.path())
    {
        let prepared = vize_s0::path::canonicalize_non_verbatim(prepared);
        for diagnostic in &mut result.diagnostics {
            if vize_s0::path::canonicalize_non_verbatim(&diagnostic.file) == prepared {
                diagnostic.file = authored.clone();
            }
        }
    }

    Ok(ProgramExecution {
        checker,
        result,
        input_files: input.files.to_vec(),
        reported_files: input.reported_files,
        tsconfig_path: input.tsconfig_path,
        program_root: input.program_root,
        import_time: Duration::ZERO,
        gen_time,
        check_time: check_start.elapsed(),
        _checker_tsconfig: checker_tsconfig,
    })
}
