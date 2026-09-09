//! Virtual project management for Corsa-backed type checking.
//!
//! This module materializes a mirrored TypeScript project in
//! a project-keyed namespace under Git storage when available, falling back to
//! `.vize/canon/`, so Corsa can type-check Vue SFCs together with
//! regular TypeScript sources, ambient declarations, and emitted `.d.ts` files.
//!
//! The implementation is split across submodules:
//!
//! - [`project`] — construction, configuration, and file registration.
//! - [`mapping`] — lookup and bidirectional position mapping.
//! - [`materialize`] — writing the virtual tree to disk.
//! - [`tsconfig_gen`] — generating the virtual `tsconfig.json`.
//! - [`build`] / [`passthrough`] — building self-contained registered files.
//! - [`vue_codegen`] / [`diagnostics`] — `.vue` virtual TS and SFC diagnostics.
//! - [`tsconfig_paths`] — tsconfig path resolution and JSONC parsing.

use std::path::PathBuf;

use vize_atelier_core::TemplateSyntaxMode;
use vize_carton::{FxHashMap, FxHashSet, String as CompactString};

use super::import_rewriter::{ImportRewriter, VirtualAliasRewritePolicy};
use super::source_map::CompositeSourceMap;
use super::source_policy::SourceFilePolicy;
use super::{Diagnostic, SfcBlockType};
use crate::virtual_ts::{VirtualTsCheckOptions, VirtualTsOptions};

mod art_usage;
mod build;
mod content_mapper;
pub use content_mapper::{
    CONTENT_MAPPER_GENERATED_DIAGNOSTIC_CODE, CONTENT_MAPPER_SFC_PARSE_ERROR_CODE,
    CONTENT_MAPPER_VIRTUAL_EXTENSION, ContentMapperDiagnostic, ContentMapperDiagnosticDirective,
    ContentMapperDiagnosticDirectives, ContentMapperSemanticLink, ContentMapperSpan,
    ContentMapperTransform, ContentMapperTransformOptions, ContentMapperUnusedExpectDiagnostic,
    generate_vue_content_mapper_transform, generate_vue_content_mapper_transform_with_options,
};
mod css_var_usage;
mod declaration_emit;
pub(crate) mod dependency_scan;
mod document;
mod incremental_graph;
pub use document::{
    VueDocumentVirtualTs, VueDocumentVirtualTsOptions, generate_vue_document_virtual_ts,
    generate_vue_document_virtual_ts_with_options,
};
// Crate-scoped: the `alias_resolver` parameter names
// `import_rewriter_alias::AliasSpecifierResolver`, which is `pub(crate)`.
pub(crate) use document::generate_vue_document_virtual_ts_with_options_and_alias_resolver;
mod diagnostics;
mod esm_declaration_spelling;
mod external_mirror;
pub use external_mirror::external_mirror_original_path;
mod identity;
pub use identity::{project_virtual_lock_paths, project_virtual_root};
mod javascript_sfc;
mod jsx_build;
mod jsx_codegen;
mod mapping;
pub(crate) use mapping::MaterializedSourceMappingKind;
mod materialize;
mod materialize_delta;
mod materialize_links;
pub(crate) use materialize_delta::{
    IncrementalMaterialization, MaterializedFileDelta, MaterializedFileSnapshot,
};
mod materialize_stubs;
pub(crate) mod option_probe;
mod package_link_owners;
mod package_node_modules;
pub(crate) mod package_resolution;
mod package_route_discovery;
pub use package_route_discovery::is_vue_runtime_support_specifier;
mod package_route_reachability;
pub(crate) use package_route_reachability::package_route_reaches_vue;
pub use package_route_reachability::{
    PACKAGE_REACHABILITY_BUDGET_REVISION, PackageRouteReachability, ReachabilityOutcome,
    ReachabilityWork, scan_package_route_reachability,
};
mod package_shadow;
mod package_shadow_owners;
mod package_shadow_runtime;
mod package_shadow_scope;
mod package_source_index;
mod passthrough;
mod paths;
mod project;
mod setup_props;
mod topology;
pub use topology::BatchTopologyMetrics;
mod tsconfig_gen;
pub use tsconfig_gen::{
    TsconfigOwnershipCache, TsconfigOwnershipOptions, TsconfigSourceKind,
    snapshot_tsconfig_compiler_options,
};
mod tsconfig_paths;
mod vue_codegen;

#[cfg(test)]
mod tests;

pub(super) const AUTO_IMPORT_STUBS_FILE: &str = "__vize_auto_imports.d.ts";
pub(super) const MODULE_AUGMENTATION_STUBS_FILE: &str = "__vize_module_augmentations.d.ts";
pub(super) const MODULE_AUGMENTATION_STUB_PREFIX: &str = "// @vize-module-augmentation\n";
pub(super) const PACKAGE_BOUNDARY_FILE: &str = "package.json";
pub(super) const VUE_MODULE_STUBS_FILE: &str = "__vize_vue_modules.d.ts";
/// Shared ambient helpers materialized once per program; generated `.vue.ts`
/// modules are emitted with their preamble hoisted into this file.
pub(super) const SHARED_HELPERS_FILE: &str = crate::virtual_ts::SHARED_PREAMBLE_FILE_NAME;

/// Ordered owners for one materialized package shadow path.
///
/// The first route key is the deterministic winner. Keeping that winner at the
/// front avoids scanning every importer whenever another route shares a shadow.
type PackageShadowOwners =
    std::collections::BTreeMap<crate::package_route::PackageRouteKey, PathBuf>;

/// A virtual file in the project.
#[derive(Debug)]
pub struct VirtualFile {
    /// Generated or rewritten source code used by Corsa.
    pub content: CompactString,
    /// Source map for position mapping.
    pub source_map: CompositeSourceMap,
    /// Original file path.
    pub original_path: PathBuf,
    /// Materialized file path inside the virtual project.
    pub virtual_path: PathBuf,
}

/// Original position after mapping.
#[derive(Debug, Clone)]
pub struct OriginalPosition {
    /// Original file path.
    pub path: PathBuf,
    /// Line number (0-based).
    pub line: u32,
    /// Column number (0-based).
    pub column: u32,
    /// SFC block type if applicable.
    pub block_type: Option<SfcBlockType>,
}

/// Virtual project for Corsa-backed type checking.
pub struct VirtualProject {
    /// Project root directory.
    project_root: PathBuf,

    /// Project-keyed materialized root. Batch projects use Git storage when
    /// available, falling back to `.vize/canon/projects`, so typechecking does
    /// not create or mutate the project's `node_modules`; editor projects are
    /// re-scoped to a session-private namespace.
    virtual_root: PathBuf,

    /// Explicit tsconfig path, if one was provided by the caller.
    tsconfig_path: Option<PathBuf>,

    /// Whether the effective tsconfig asks TypeScript to report unused symbols.
    preserve_unused_diagnostics: bool,

    /// Effective TypeScript source membership and JavaScript diagnostic policy.
    source_policy: SourceFilePolicy,

    /// Effective path-alias policy for virtual `.vue` specifier rewrites.
    alias_rewrite_policy: VirtualAliasRewritePolicy,

    /// Global virtual TS options applied to every Vue file.
    virtual_ts_options: VirtualTsOptions,

    /// Authored program files diagnosed in place instead of materialized.
    diagnostic_paths: FxHashSet<PathBuf>,

    /// Caller-selected source roots eligible for declaration output. Inferred
    /// package dependencies participate in checking but must not be published
    /// under the virtual external-mirror layout.
    declaration_roots: Option<FxHashSet<PathBuf>>,

    /// Internal check generation settings applied to every Vue file.
    virtual_ts_check_options: VirtualTsCheckOptions,

    /// Importer-local package identities retained until native package
    /// topology is materialized. This must never collapse to a specifier map.
    package_routes: FxHashMap<crate::package_route::PackageRouteKey, crate::PackageRouteBinding>,

    /// Reverse invalidation and importer indexes for persistent refreshes.
    /// These live beside the authoritative bindings so a warm check never has
    /// to clone/sort the whole workspace route set merely to find one package.
    package_route_reverse: FxHashMap<PathBuf, FxHashSet<crate::package_route::PackageRouteKey>>,
    package_route_importers: FxHashMap<PathBuf, FxHashSet<crate::package_route::PackageRouteKey>>,
    package_route_source_owners: FxHashMap<PathBuf, usize>,
    package_route_manifests: FxHashMap<PathBuf, FxHashSet<crate::package_route::PackageRouteKey>>,
    package_route_roots: FxHashMap<PathBuf, FxHashSet<crate::package_route::PackageRouteKey>>,
    package_source_index: FxHashMap<PathBuf, FxHashMap<PathBuf, (PathBuf, PathBuf)>>,

    /// Authored dependency ownership for persistent source membership.
    dependency_edges: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    dependency_inbound: FxHashMap<PathBuf, usize>,

    /// Shared self-invalidating resolver used when persistent snapshots refresh
    /// a package manifest, link, lockfile, or source without restarting Corsa.
    #[allow(clippy::disallowed_types)]
    package_route_resolver: std::sync::Arc<std::sync::Mutex<crate::PackageRouteResolver>>,
    package_routes_need_refresh: bool,
    package_route_refresh_keys: FxHashSet<crate::package_route::PackageRouteKey>,

    /// Shadow path -> canonical generated file. Shadows reuse the canonical
    /// source map and never become authored/publishable identities.
    package_shadow_files: FxHashMap<PathBuf, PathBuf>,

    /// Editor-only selector modules. A selector keeps the authored bare
    /// spelling and lets native TypeScript resolve the copied raw manifest.

    /// Shadow package.json -> original package.json. Manifests are copied raw.
    package_shadow_manifests: FxHashMap<PathBuf, PathBuf>,
    package_shadow_artifacts:
        FxHashMap<crate::package_route::PackageRouteKey, package_shadow::PackageShadowTopology>,
    package_shadow_file_owners: FxHashMap<PathBuf, PackageShadowOwners>,
    package_shadow_manifest_owners: FxHashMap<PathBuf, PackageShadowOwners>,
    package_shadow_source_paths: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    package_shadow_dirty_keys: FxHashSet<crate::package_route::PackageRouteKey>,
    package_shadows_initialized: bool,

    /// Package name -> the directories whose `node_modules` host the shadow
    /// scopes shared by every importer that resolved that name to the same
    /// physical package. Names without a single project-wide identity are
    /// absent and keep their per-importer scopes (#4153).
    package_shadow_scopes: FxHashMap<CompactString, Vec<PathBuf>>,

    /// Materialized paths touched by the next persistent patch. Cold
    /// materialization clears this set; warm checks write/hash only these
    /// entries and never walk the virtual tree.
    incremental_materialized_candidates: FxHashSet<PathBuf>,
    incremental_source_nodes_rebuilt: usize,
    incremental_dependency_nodes_reconciled: usize,
    incremental_shadow_bindings_rebuilt: usize,
    /// Exact package dependency links materialized by the persistent checker.
    /// Each logical link scope owns only the paths below that importer/package
    /// boundary, so a warm route patch never rescans unrelated source roots.
    materialized_package_links: FxHashMap<PathBuf, PathBuf>,
    materialized_package_link_scopes: FxHashMap<PathBuf, FxHashMap<PathBuf, PathBuf>>,
    materialized_package_link_owners: FxHashMap<PathBuf, FxHashMap<PathBuf, PathBuf>>,
    package_link_scope_files: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
    package_link_scope_targets: FxHashMap<PathBuf, FxHashMap<PathBuf, usize>>,
    package_shadow_link_scopes:
        FxHashMap<crate::package_route::PackageRouteKey, Vec<(PathBuf, PathBuf)>>,
    incremental_package_link_scopes: FxHashSet<PathBuf>,
    incremental_link_topology_dirty: bool,

    /// Enable Vue 2.7 / Nuxt 2 Options API compatibility for virtual files.
    options_api: bool,

    /// Editor sessions register reachable in-root scripts too: they have no
    /// generated tsconfig aliasing `.vue` imports into the mirror, so a barrel
    /// like `src/Primitive/index.ts` must be mirrored for its `.vue`
    /// re-exports to rewrite (#3915). Batch keeps the narrow set that
    /// incremental sessions and Tier-L pin (#3898).
    session_scripts: bool,
    legacy_vue2: bool,

    /// Opt-in type-checking of `.jsx`/`.tsx` Vue components (#1497). Default-off:
    /// when disabled, `.jsx`/`.tsx` are passed to TypeScript verbatim (React
    /// passthrough); when enabled, they are routed through the Vize JSX
    /// virtual-TS path.
    jsx_typecheck: bool,

    /// Configured Vue dialect from `vue.version` (default [`VueVersion::V3`]).
    ///
    /// Threaded into virtual-TS generation for dialect-aware Vue instance
    /// typing.
    dialect: vize_carton::config::VueVersion,

    /// Template syntax compatibility used when parsing SFC templates.
    template_syntax: TemplateSyntaxMode,

    /// Enable experimental Vue in-tag comments while parsing SFC templates.
    experimental_in_tag_comments: bool,

    /// Virtual files keyed by materialized path.
    virtual_files: FxHashMap<PathBuf, VirtualFile>,

    /// Exact materialized artifacts owned by each authored source. Persistent
    /// refreshes replace this set atomically so an SFC changing TS/TSX shape
    /// cannot leave an old companion or passthrough file in the program.
    source_artifacts: FxHashMap<PathBuf, SourceArtifacts>,

    /// Non-TS module files that must exist in the virtual mirror for TypeScript
    /// module resolution, keyed by materialized path.
    passthrough_files: FxHashMap<PathBuf, PathBuf>,

    /// Secondary index: original source path -> materialized (virtual) path.
    /// Keeps `find_by_original` / `map_to_virtual` O(1) instead of scanning
    /// every virtual file on each LSP position-mapping request.
    original_index: FxHashMap<PathBuf, PathBuf>,

    /// Original source text as registered, keyed by materialized (virtual)
    /// path. Retained here (rather than on the public `VirtualFile`) so
    /// position mapping can convert offsets to line/column without re-reading
    /// the file from disk on every request, and without changing the public
    /// `VirtualFile` API. This is also the exact text the source map's
    /// original offsets refer to (e.g. an editor's unsaved buffer), so it is
    /// more correct than re-reading live disk state.
    original_contents: FxHashMap<PathBuf, CompactString>,

    /// Materialized paths of `.vue` files whose script block is JavaScript.
    /// TypeScript diagnostics mapped back into these files are dropped unless
    /// the project enables `checkJs` (see [`javascript_sfc`], #3322).
    unchecked_javascript_files: FxHashSet<PathBuf>,

    /// Parser diagnostics collected before Corsa runs.
    diagnostics: Vec<Diagnostic>,

    /// Import rewriter for `.vue` specifiers inside TypeScript sources.
    rewriter: ImportRewriter,
}

#[derive(Default)]
struct SourceArtifacts {
    virtual_paths: Vec<PathBuf>,
    passthrough_paths: Vec<PathBuf>,
}
