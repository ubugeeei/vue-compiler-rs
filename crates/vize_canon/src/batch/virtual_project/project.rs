//! `VirtualProject` lifecycle: construction, configuration, and file
//! registration. Registration delegates the expensive per-file work to
//! [`super::build`] so it can run in parallel, then absorbs the results into
//! the project's indexes.

use std::path::{Path, PathBuf};

use oxc_span::SourceType;
use rayon::prelude::*;
use vize_atelier_core::TemplateSyntaxMode;
use vize_carton::{FxHashMap, FxHashSet, profile};

use crate::batch::error::{CorsaError, CorsaResult};
use crate::batch::import_rewriter::ImportRewriter;
use crate::virtual_ts::{VirtualTsCheckOptions, VirtualTsOptions};

mod artifacts;
mod config;
mod package_routes;

use super::build::{
    RegisteredFile, VirtualBuildContext, build_registered_file, build_script_registered_file,
    build_vue_registered_file, source_type_for_path,
};
use super::setup_props::RuntimePropResolveCache;
use super::{VirtualProject, project_virtual_root};

impl VirtualProject {
    /// Create a new virtual project.
    pub fn new(project_root: &Path) -> CorsaResult<Self> {
        let project_root = vize_carton::path::canonicalize_non_verbatim(project_root);
        let virtual_root = project_virtual_root(&project_root);

        let mut project = Self {
            project_root,
            virtual_root,
            tsconfig_path: None,
            preserve_unused_diagnostics: false,
            source_policy: Default::default(),
            alias_rewrite_policy: Default::default(),
            virtual_ts_options: VirtualTsOptions::default(),
            diagnostic_paths: FxHashSet::default(),
            declaration_roots: None,
            virtual_ts_check_options: VirtualTsCheckOptions::default(),
            package_routes: FxHashMap::default(),
            package_route_reverse: FxHashMap::default(),
            package_route_importers: FxHashMap::default(),
            package_route_source_owners: FxHashMap::default(),
            package_route_manifests: FxHashMap::default(),
            package_route_roots: FxHashMap::default(),
            package_source_index: FxHashMap::default(),
            dependency_edges: FxHashMap::default(),
            dependency_inbound: FxHashMap::default(),
            package_route_resolver: Default::default(),
            package_routes_need_refresh: false,
            package_route_refresh_keys: FxHashSet::default(),
            package_shadow_files: FxHashMap::default(),
            package_shadow_manifests: FxHashMap::default(),
            package_shadow_artifacts: FxHashMap::default(),
            package_shadow_file_owners: FxHashMap::default(),
            package_shadow_manifest_owners: FxHashMap::default(),
            package_shadow_source_paths: FxHashMap::default(),
            package_shadow_dirty_keys: FxHashSet::default(),
            package_shadows_initialized: false,
            package_shadow_scopes: FxHashMap::default(),
            incremental_materialized_candidates: FxHashSet::default(),
            incremental_source_nodes_rebuilt: 0,
            incremental_dependency_nodes_reconciled: 0,
            incremental_shadow_bindings_rebuilt: 0,
            materialized_package_links: FxHashMap::default(),
            materialized_package_link_scopes: FxHashMap::default(),
            materialized_package_link_owners: FxHashMap::default(),
            package_link_scope_files: FxHashMap::default(),
            package_link_scope_targets: FxHashMap::default(),
            package_shadow_link_scopes: FxHashMap::default(),
            incremental_package_link_scopes: FxHashSet::default(),
            incremental_link_topology_dirty: false,
            options_api: false,
            session_scripts: false,
            legacy_vue2: false,
            jsx_typecheck: false,
            dialect: vize_carton::config::VueVersion::default(),
            template_syntax: TemplateSyntaxMode::default(),
            experimental_in_tag_comments: false,
            virtual_files: FxHashMap::default(),
            source_artifacts: FxHashMap::default(),
            passthrough_files: FxHashMap::default(),
            original_index: FxHashMap::default(),
            original_contents: FxHashMap::default(),
            unchecked_javascript_files: FxHashSet::default(),
            diagnostics: Vec::new(),
            rewriter: ImportRewriter::new(),
        };
        project.preserve_unused_diagnostics =
            project.resolve_tsconfig_preserves_unused_diagnostics();
        project.source_policy = project.resolve_source_file_policy();
        project.alias_rewrite_policy = project.resolve_alias_rewrite_policy();
        Ok(project)
    }

    /// Register a supported file path.
    pub(super) fn rewriter(&self) -> &crate::batch::import_rewriter::ImportRewriter {
        &self.rewriter
    }

    pub fn register_path(&mut self, path: &Path) -> CorsaResult<()> {
        let content = profile!("canon.file.read", std::fs::read_to_string(path))?;
        self.register_path_with_content(path, &content)
    }

    /// Register a supported file path with already-loaded content.
    pub fn register_path_with_content(&mut self, path: &Path, content: &str) -> CorsaResult<()> {
        let package_route_path = self.is_package_route_path(path);
        let registered = build_registered_file(
            path,
            content,
            VirtualBuildContext {
                project_root: &self.project_root,
                virtual_root: &self.virtual_root,
                virtual_ts_options: &self.virtual_ts_options,
                virtual_ts_check_options: self.virtual_ts_check_options,
                preserve_unused_diagnostics: self.tsconfig_preserves_unused_diagnostics(),
                options_api: self.options_api,
                legacy_vue2: self.legacy_vue2,
                jsx_typecheck: self.jsx_typecheck,
                dialect: self.dialect,
                template_syntax: self.template_syntax,
                experimental_in_tag_comments: self.experimental_in_tag_comments,
                hoist_shared_preamble: true,
                preserve_relative_declarations: package_route_path,
                preserve_declaration_spelling: self.session_scripts,
                rewriter: &self.rewriter,
                alias_rewrite_policy: Some(self.alias_rewrite_policy()),
                runtime_prop_resolve_cache: None,
            },
        )?;
        self.absorb_registered_file(registered);
        Ok(())
    }

    /// Register a batch of file paths, parallelizing per-file parse and Virtual TS
    /// generation across rayon's thread pool. Falls back to sequential work when
    /// the batch is small enough that the fan-out cost would dominate.
    ///
    /// This is deliberately structured as "parallel build, sequential absorb".
    /// `build_registered_file` owns the expensive work (disk read, SFC parse,
    /// template parse, virtual-TS generation, import rewriting) and only needs an
    /// immutable build context, so it scales cleanly across rayon workers. The
    /// mutable project indexes are updated after the join point, which preserves
    /// deterministic maps and avoids locking every insertion in the hot loop.
    pub fn register_paths(&mut self, paths: &[PathBuf]) -> CorsaResult<()> {
        let valid_paths: Vec<&Path> = paths
            .iter()
            .filter(|path| path.is_file())
            .map(PathBuf::as_path)
            .collect();
        if valid_paths.is_empty() {
            return Ok(());
        }

        // Sequential is cheaper for tiny batches than firing up rayon workers.
        if valid_paths.len() <= 1 {
            for path in valid_paths {
                self.register_path(path)?;
            }
            return Ok(());
        }

        let preserve_unused_diagnostics = self.tsconfig_preserves_unused_diagnostics();
        let runtime_prop_resolve_cache = RuntimePropResolveCache::default();
        let build_context = VirtualBuildContext {
            project_root: self.project_root.as_path(),
            virtual_root: self.virtual_root.as_path(),
            virtual_ts_options: &self.virtual_ts_options,
            virtual_ts_check_options: self.virtual_ts_check_options,
            preserve_unused_diagnostics,
            options_api: self.options_api,
            legacy_vue2: self.legacy_vue2,
            jsx_typecheck: self.jsx_typecheck,
            dialect: self.dialect,
            template_syntax: self.template_syntax,
            experimental_in_tag_comments: self.experimental_in_tag_comments,
            hoist_shared_preamble: true,
            preserve_relative_declarations: false,
            preserve_declaration_spelling: self.session_scripts,
            rewriter: &self.rewriter,
            alias_rewrite_policy: Some(self.alias_rewrite_policy()),
            runtime_prop_resolve_cache: Some(&runtime_prop_resolve_cache),
        };
        let package_paths = self
            .package_routes
            .values()
            .filter_map(|binding| binding.route.as_ref())
            .flat_map(|route| route.all_source_paths().into_iter().cloned())
            .collect::<FxHashSet<_>>();

        let registered: Result<Vec<RegisteredFile>, CorsaError> = valid_paths
            .par_iter()
            .map(|&path| {
                let content = profile!("canon.file.read", std::fs::read_to_string(path))?;
                let mut context = build_context;
                context.preserve_relative_declarations = package_paths.contains(path);
                build_registered_file(path, &content, context)
            })
            .collect();

        self.virtual_files.reserve(valid_paths.len());
        for registered in registered? {
            self.absorb_registered_file(registered);
        }
        Ok(())
    }

    /// Register a `.vue` file.
    pub fn register_vue_file(&mut self, path: &Path, content: &str) -> CorsaResult<()> {
        let registered = build_vue_registered_file(
            path,
            content,
            VirtualBuildContext {
                project_root: &self.project_root,
                virtual_root: &self.virtual_root,
                virtual_ts_options: &self.virtual_ts_options,
                virtual_ts_check_options: self.virtual_ts_check_options,
                preserve_unused_diagnostics: self.tsconfig_preserves_unused_diagnostics(),
                options_api: self.options_api,
                legacy_vue2: self.legacy_vue2,
                jsx_typecheck: self.jsx_typecheck,
                dialect: self.dialect,
                template_syntax: self.template_syntax,
                experimental_in_tag_comments: self.experimental_in_tag_comments,
                hoist_shared_preamble: true,
                preserve_relative_declarations: self.is_package_route_path(path),
                preserve_declaration_spelling: self.session_scripts,
                rewriter: &self.rewriter,
                alias_rewrite_policy: Some(self.alias_rewrite_policy()),
                runtime_prop_resolve_cache: None,
            },
        )?;
        self.absorb_registered_file(registered);
        Ok(())
    }

    /// Register a `.ts`/`.tsx`/`.mts`/`.cts` file.
    pub fn register_ts_file(&mut self, path: &Path) -> CorsaResult<()> {
        let content = std::fs::read_to_string(path)?;
        let source_type = source_type_for_path(path).ok_or_else(|| CorsaError::PathError {
            path: path.to_path_buf(),
        })?;
        self.register_script_file(path, &content, source_type)
    }

    /// Register a `.d.ts` file.
    pub fn register_declaration_file(&mut self, path: &Path, content: &str) -> CorsaResult<()> {
        self.register_script_file(path, content, SourceType::ts())
    }

    /// Register a non-Vue source file.
    pub fn register_script_file(
        &mut self,
        path: &Path,
        content: &str,
        source_type: SourceType,
    ) -> CorsaResult<()> {
        let registered = build_script_registered_file(
            path,
            content,
            source_type,
            (&self.project_root, &self.virtual_root),
            &self.rewriter,
            Some(self.alias_rewrite_policy()),
            self.is_package_route_path(path),
            self.session_scripts,
        )?;
        self.absorb_registered_file(registered);
        Ok(())
    }
}
