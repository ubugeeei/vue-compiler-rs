//! Vue virtual-document synchronization for editor Corsa sessions.

use std::path::{Path, PathBuf};

use vize_carton::{FxHashMap, String, cstr};

use super::bridge::CorsaBridge;
use super::types::CorsaBridgeError;
use super::vue_dependencies::{collect_dependency_documents, tsx_vue_import_shim};
use crate::batch::{ImportRewriter, VueDocumentVirtualTsOptions};
use crate::file_uri::path_to_file_uri;
use crate::virtual_ts::VirtualTsOptions;

#[path = "vue_document/types.rs"]
mod model;
pub(crate) use model::CorsaVueVirtualProject;
pub(super) use model::GeneratedVueDocument;
pub use model::{
    CorsaMaterializedMappingKind, CorsaMaterializedSource, CorsaVueVirtualDependency,
    CorsaVueVirtualDocument, CorsaVueVirtualDocumentOptions,
};

#[derive(Clone, Copy)]
pub(crate) struct CorsaProjectEnvironment<'a> {
    pub(crate) virtual_ts_options: &'a VirtualTsOptions,
    pub(crate) package_routes: &'a crate::PackageRouteResolver,
    pub(crate) project_root: Option<&'a Path>,
    pub(crate) tsconfig_path: Option<&'a Path>,
    pub(crate) editor_session: &'a crate::corsa_bridge::EditorMirrorSession,
}

impl CorsaBridge {
    /// Remove virtual TypeScript overlays derived from deleted Vue SFCs.
    pub async fn forget_vue_virtual_documents(
        &self,
        source_paths: &[PathBuf],
    ) -> Result<(), CorsaBridgeError> {
        super::vue_dependencies_alias::AliasContext::forget_cached_sources(
            &self.editor_session,
            source_paths,
        );
        let source_paths = source_paths.to_vec();
        self.with_client(move |client| {
            client
                .forget_vue_virtual_documents(&source_paths)
                .map_err(CorsaBridgeError::CommunicationError)
        })
        .await
    }

    /// Generate, sync, and return the canonical `.vue.{ts,tsx}` document used
    /// for editor diagnostics, hover, definition, references, and rename.
    pub async fn open_vue_virtual_document(
        &self,
        source_path: &Path,
        content: &str,
        options: CorsaVueVirtualDocumentOptions,
    ) -> Result<CorsaVueVirtualDocument, CorsaBridgeError> {
        self.open_vue_virtual_document_with_overlays(source_path, content, options, &[])
            .await
    }

    /// Generate and sync a Vue document while preferring unsaved dependency
    /// buffers over their on-disk contents.
    pub async fn open_vue_virtual_document_with_overlays(
        &self,
        source_path: &Path,
        content: &str,
        options: CorsaVueVirtualDocumentOptions,
        overlays: &[(PathBuf, String)],
    ) -> Result<CorsaVueVirtualDocument, CorsaBridgeError> {
        self.open_vue_virtual_document_with_overlays_and_options(
            source_path,
            content,
            options,
            overlays,
            &VirtualTsOptions::default(),
        )
        .await
    }

    /// Generate and sync a Vue document with editor-specific virtual-TS options.
    pub async fn open_vue_virtual_document_with_overlays_and_options(
        &self,
        source_path: &Path,
        content: &str,
        options: CorsaVueVirtualDocumentOptions,
        overlays: &[(PathBuf, String)],
        virtual_ts_options: &VirtualTsOptions,
    ) -> Result<CorsaVueVirtualDocument, CorsaBridgeError> {
        let overlays = overlays
            .iter()
            .map(|(path, content)| (path.clone(), content.as_str()))
            .collect::<Vec<_>>();
        self.open_vue_virtual_document_with_borrowed_overlays_and_options(
            source_path,
            content,
            options,
            &overlays,
            virtual_ts_options,
        )
        .await
    }

    /// Generate and sync a Vue document without copying unchanged overlay text.
    ///
    /// Only dependency entries reachable from the host's imports are read, so
    /// callers with shared buffer snapshots can lend their text for this call.
    pub async fn open_vue_virtual_document_with_borrowed_overlays_and_options(
        &self,
        source_path: &Path,
        content: &str,
        options: CorsaVueVirtualDocumentOptions,
        overlays: &[(PathBuf, &str)],
        virtual_ts_options: &VirtualTsOptions,
    ) -> Result<CorsaVueVirtualDocument, CorsaBridgeError> {
        let project = build_vue_virtual_project_with_overlays_and_options_and_package_routes(
            source_path,
            content,
            options,
            overlays,
            CorsaProjectEnvironment {
                virtual_ts_options,
                package_routes: &self.package_route_resolver,
                project_root: self.config.working_dir.as_deref(),
                tsconfig_path: self.config.tsconfig_path.as_deref(),
                editor_session: &self.editor_session,
            },
        )?;
        let CorsaVueVirtualProject {
            host,
            documents,
            session_project_root,
            materialized_changes,
        } = project;
        self.open_canon_project_documents(&documents, session_project_root, materialized_changes)
            .await?;
        Ok(host)
    }

    pub(super) async fn open_canon_project_documents(
        &self,
        documents: &[(String, String)],
        session_project_root: Option<PathBuf>,
        materialized_changes: crate::batch::virtual_project::MaterializedFileDelta,
    ) -> Result<(), CorsaBridgeError> {
        if let Some(project_root) = session_project_root {
            self.with_client(move |client| {
                client
                    .synchronize_materialized_project(&project_root, &materialized_changes)
                    .map_err(CorsaBridgeError::CommunicationError)
            })
            .await?;
        }
        self.open_virtual_documents_batch(documents).await
    }
}

#[cfg(test)]
pub(crate) fn build_vue_virtual_project(
    source_path: &Path,
    content: &str,
    options: CorsaVueVirtualDocumentOptions,
) -> Result<CorsaVueVirtualProject, CorsaBridgeError> {
    build_vue_virtual_project_with_overlays(source_path, content, options, &[])
}

#[cfg(test)]
pub(crate) fn build_vue_virtual_project_with_overlays(
    source_path: &Path,
    content: &str,
    options: CorsaVueVirtualDocumentOptions,
    overlays: &[(PathBuf, &str)],
) -> Result<CorsaVueVirtualProject, CorsaBridgeError> {
    build_vue_virtual_project_with_overlays_and_options(
        source_path,
        content,
        options,
        overlays,
        &VirtualTsOptions::default(),
    )
}

#[cfg(test)]
fn build_vue_virtual_project_with_overlays_and_options(
    source_path: &Path,
    content: &str,
    options: CorsaVueVirtualDocumentOptions,
    overlays: &[(PathBuf, &str)],
    virtual_ts_options: &VirtualTsOptions,
) -> Result<CorsaVueVirtualProject, CorsaBridgeError> {
    build_vue_virtual_project_with_overlays_and_options_and_package_routes(
        source_path,
        content,
        options,
        overlays,
        CorsaProjectEnvironment {
            virtual_ts_options,
            package_routes: &crate::PackageRouteResolver::default(),
            project_root: None,
            tsconfig_path: None,
            editor_session: super::editor_session::fallback_editor_session(),
        },
    )
}

pub(crate) fn build_vue_virtual_project_with_overlays_and_options_and_package_routes(
    source_path: &Path,
    content: &str,
    options: CorsaVueVirtualDocumentOptions,
    overlays: &[(PathBuf, &str)],
    environment: CorsaProjectEnvironment<'_>,
) -> Result<CorsaVueVirtualProject, CorsaBridgeError> {
    let rewriter = ImportRewriter::new();
    let overlays = overlays
        .iter()
        .map(|(path, content)| {
            let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            (key, *content)
        })
        .collect::<FxHashMap<_, _>>();
    // The alias mirror is built before generation, and from the same buffers the
    // dependency walk reads, so a specifier the resolver rewrites always has a
    // materialized target (#3900).
    let alias_context = super::vue_dependencies_alias::AliasContext::for_host_cached(
        source_path,
        content,
        &overlays,
        options,
        environment,
    )?;
    let host = generate_vue_document_with_options(
        source_path,
        content,
        options,
        environment.virtual_ts_options,
        &rewriter,
        Some(&alias_context),
    )?;
    let mut documents = vec![(host.virtual_uri.clone(), host.generated.code.clone())];
    let mut dependencies = Vec::new();
    if host.generated.virtual_suffix == ".tsx" {
        documents.push(tsx_vue_import_shim(&host.source_path, &host.virtual_uri));
    }
    collect_dependency_documents(
        &mut documents,
        &mut dependencies,
        &host,
        options,
        &rewriter,
        &alias_context,
        &overlays,
    );
    let generated = host.generated;
    let materialized_sources = alias_context.materialized_sources();
    let session_project_root = alias_context.mirror_project_root_for_source(source_path);
    let materialized_changes = alias_context.materialized_changes.clone();
    Ok(CorsaVueVirtualProject {
        host: CorsaVueVirtualDocument {
            request_uri: host.virtual_uri,
            code: generated.code,
            pre_rewrite_code: generated.pre_rewrite_code,
            mappings: generated.mappings,
            semantic_links: generated.semantic_links,
            import_source_map: generated.import_source_map,
            source_type: generated.source_type,
            virtual_suffix: generated.virtual_suffix,
            dependencies,
            materialized_sources,
            session_project_root: session_project_root.clone(),
        },
        documents,
        session_project_root,
        materialized_changes,
    })
}
/// Generate a Vue document with alias-aware import rewriting: non-relative
/// specifiers the context resolves are pointed at the synced overlay
/// identities through the offset-preserving rewriter (#3900).
pub(super) fn generate_vue_document_with_alias(
    source_path: &Path,
    content: &str,
    options: CorsaVueVirtualDocumentOptions,
    rewriter: &ImportRewriter,
    context: &super::vue_dependencies_alias::AliasContext,
) -> Result<GeneratedVueDocument, CorsaBridgeError> {
    generate_vue_document_with_options(
        source_path,
        content,
        options,
        context.virtual_ts_options(),
        rewriter,
        Some(context),
    )
}

fn generate_vue_document_with_options(
    source_path: &Path,
    content: &str,
    options: CorsaVueVirtualDocumentOptions,
    virtual_ts_options: &VirtualTsOptions,
    rewriter: &ImportRewriter,
    alias_context: Option<&super::vue_dependencies_alias::AliasContext>,
) -> Result<GeneratedVueDocument, CorsaBridgeError> {
    let source_dir = source_path.parent().map(std::path::Path::to_path_buf);
    let alias_resolver = alias_context.zip(source_dir).map(|(context, dir)| {
        move |specifier: &str, mode| context.resolve_specifier_to_mirror_path(specifier, &dir, mode)
    });
    let generated = crate::batch::virtual_project::generate_vue_document_virtual_ts_with_options_and_alias_resolver(
        source_path,
        content,
        virtual_ts_options,
        rewriter,
        false,
        VueDocumentVirtualTsOptions {
            options_api: options.options_api,
            legacy_vue2: options.legacy_vue2,
            preserve_event_navigation: options.preserve_event_navigation,
            dialect: options.dialect,
            preserve_missing_vue_diagnostics: true,
        },
        alias_resolver
            .as_ref()
            .map(|resolver| resolver as crate::batch::import_rewriter_alias::AliasSpecifierResolver<'_>),
    )
    .map_err(|error| CorsaBridgeError::CommunicationError(cstr!("{error}")))?;
    let virtual_path = alias_context
        .and_then(|context| context.mirror_virtual_path(source_path))
        .unwrap_or_else(|| {
            source_path.with_file_name(cstr!(
                "{}{}",
                source_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default(),
                generated.virtual_suffix
            ))
        });
    let virtual_uri = path_to_file_uri(&virtual_path);

    Ok(GeneratedVueDocument {
        source_path: source_path.to_path_buf(),
        virtual_uri,
        generated,
    })
}
