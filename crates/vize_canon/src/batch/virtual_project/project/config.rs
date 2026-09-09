//! `VirtualProject` configuration: generation options, declaration roots, and
//! virtual module aliases carried into virtual-TS generation.

use std::path::{Path, PathBuf};

use vize_atelier_core::TemplateSyntaxMode;

use crate::batch::source_policy::SourceFilePolicy;
use crate::virtual_ts::{VirtualTsCheckOptions, VirtualTsOptions};

use super::super::VirtualProject;

impl VirtualProject {
    pub(crate) fn scope_editor_namespace(&mut self, storage_root: &Path, identity: u64) {
        debug_assert!(self.virtual_files.is_empty());
        debug_assert!(self.package_routes.is_empty());
        self.virtual_root = super::super::identity::project_virtual_root_with_identity(
            storage_root,
            &self.project_root,
            identity,
        );
    }

    pub(crate) fn use_effective_tsconfig_for_source(&mut self, source_path: &Path) {
        let Some(shell) = self.resolved_tsconfig_path() else {
            return;
        };
        let effective = super::super::tsconfig_gen::references::effective_config_for_source(
            &shell,
            source_path,
        );
        self.set_tsconfig_path(Some(effective));
    }

    pub(crate) fn effective_tsconfig_path(&self) -> Option<PathBuf> {
        self.resolved_tsconfig_path()
    }

    /// Set the tsconfig path to extend.
    pub fn set_tsconfig_path(&mut self, tsconfig_path: Option<PathBuf>) {
        self.tsconfig_path = tsconfig_path.map(vize_carton::path::normalize_windows_verbatim_path);
        self.preserve_unused_diagnostics = self.resolve_tsconfig_preserves_unused_diagnostics();
        self.source_policy = self.resolve_source_file_policy();
        self.alias_rewrite_policy = self.resolve_alias_rewrite_policy();
        self.virtual_ts_check_options.check_unknown_props = self.resolve_check_unknown_props();
    }

    pub(crate) fn source_file_policy(&self) -> SourceFilePolicy {
        self.source_policy
    }

    /// Re-read the effective compiler configuration after a watcher reports an
    /// input in the governing tsconfig/extends/reference chain.  Callers still
    /// own the graph rebuild: changing `paths`, package conditions, or JS
    /// membership is a topology event, not an in-place option tweak.
    pub(crate) fn refresh_compiler_configuration(&mut self) {
        self.preserve_unused_diagnostics = self.resolve_tsconfig_preserves_unused_diagnostics();
        self.source_policy = self.resolve_source_file_policy();
        self.alias_rewrite_policy = self.resolve_alias_rewrite_policy();
        self.virtual_ts_check_options.check_unknown_props = self.resolve_check_unknown_props();
        self.mark_incremental_config_file();
        self.mark_incremental_link_topology();
    }

    pub(crate) fn configuration_inputs_changed(&self, changed: &[PathBuf]) -> bool {
        let inputs = self.governing_config_paths();
        changed.iter().any(|path| {
            let logical = if path.is_absolute() {
                path.clone()
            } else {
                self.project_root.join(path)
            };
            let canonical = crate::package_route::stamp::canonicalize_changed_path(&logical);
            inputs.iter().any(|input| {
                input == &logical
                    || input == &canonical
                    || crate::package_route::stamp::canonicalize_changed_path(input) == canonical
            })
        })
    }

    pub(super) fn resolve_source_file_policy(&self) -> SourceFilePolicy {
        let Some(tsconfig_path) = self.resolved_tsconfig_path() else {
            return SourceFilePolicy::default();
        };
        self.load_compiler_options(Some(tsconfig_path.as_path()))
            .ok()
            .as_ref()
            .map(SourceFilePolicy::from_compiler_options)
            .unwrap_or_default()
    }

    /// Whether TypeScript diagnostics landing in `virtual_path` must be dropped
    /// because the file is a JavaScript SFC and the project does not enable
    /// `checkJs` (#3322).
    pub(crate) fn skips_typescript_diagnostics(&self, virtual_path: &Path) -> bool {
        let canonical = self
            .package_shadow_files
            .get(virtual_path)
            .map_or(virtual_path, PathBuf::as_path);
        !self.source_policy.checks_javascript()
            && self.unchecked_javascript_files.contains(canonical)
    }

    /// Set the shared virtual TS options.
    pub fn set_virtual_ts_options(&mut self, options: VirtualTsOptions) {
        self.virtual_ts_options = options;
    }

    pub(crate) fn set_virtual_ts_check_options(&mut self, mut options: VirtualTsCheckOptions) {
        options.check_unknown_props = self.virtual_ts_check_options.check_unknown_props;
        self.virtual_ts_check_options = options;
    }

    pub(crate) fn set_package_routes(
        &mut self,
        routes: impl IntoIterator<Item = crate::PackageRouteBinding>,
    ) {
        let resolution = self.package_resolution_settings();
        let mut resolver = self
            .package_route_resolver
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let bindings = routes
            .into_iter()
            .map(|mut binding| {
                let (context, context_inputs) = resolution.context(
                    &mut resolver,
                    &binding.importer_path,
                    binding.occurrence_mode,
                );
                binding.context = context;
                binding
                    .invalidation_paths
                    .extend(resolution.input_paths().iter().cloned());
                binding.invalidation_paths.extend(context_inputs);
                binding
                    .invalidation_paths
                    .push(binding.importer_path.clone());
                binding.invalidation_paths.sort();
                binding.invalidation_paths.dedup();
                binding
            })
            .collect::<Vec<_>>();
        drop(resolver);
        self.replace_package_route_bindings(bindings);
        self.package_routes_need_refresh = false;
        self.package_route_refresh_keys.clear();
    }

    // VirtualProject and its editor/check-server owners have independent
    // lifetimes, while the resolver must retain one shared cache identity.
    #[allow(clippy::disallowed_types)]
    pub(crate) fn set_package_route_resolver(&mut self, resolver: crate::PackageRouteResolver) {
        self.package_route_resolver = std::sync::Arc::new(std::sync::Mutex::new(resolver));
    }

    pub(crate) fn set_declaration_roots(&mut self, paths: &[PathBuf]) {
        let roots = paths
            .iter()
            .filter(|path| path.is_file())
            .map(|path| vize_carton::path::canonicalize_non_verbatim(path))
            .collect();
        if self.declaration_roots.as_ref() != Some(&roots) {
            self.mark_incremental_config_file();
        }
        self.declaration_roots = Some(roots);
    }

    pub(crate) fn is_declaration_root(&self, original_path: &Path) -> bool {
        self.declaration_roots
            .as_ref()
            .is_none_or(|roots| roots.contains(original_path))
    }

    pub(crate) fn set_options_api(&mut self, enabled: bool) {
        self.options_api = enabled;
    }

    pub(crate) fn set_legacy_vue2(&mut self, enabled: bool) {
        self.legacy_vue2 = enabled;
    }

    /// Enable opt-in type-checking of `.jsx`/`.tsx` Vue components (#1497).
    pub(crate) fn set_jsx_typecheck(&mut self, enabled: bool) {
        self.jsx_typecheck = enabled;
    }

    /// Set the configured Vue dialect (default [`VueVersion::V3`]).
    ///
    /// Carried into virtual-TS generation for dialect-aware instance and helper
    /// typing while keeping default-V3 output stable.
    pub(crate) fn set_dialect(&mut self, dialect: vize_carton::config::VueVersion) {
        self.dialect = dialect;
    }

    pub(crate) fn uses_shared_helpers(&self) -> bool {
        !self.legacy_vue2
            && !matches!(
                self.dialect,
                vize_carton::config::VueVersion::V2 | vize_carton::config::VueVersion::V2_7
            )
    }

    pub(crate) fn set_template_syntax(&mut self, template_syntax: TemplateSyntaxMode) {
        self.template_syntax = template_syntax;
    }

    pub(crate) fn set_experimental_in_tag_comments(&mut self, enabled: bool) {
        self.experimental_in_tag_comments = enabled;
    }

    /// Get the project root.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the virtual root.
    pub fn virtual_root(&self) -> &Path {
        &self.virtual_root
    }
}
