//! Workspace file-event handling used by LSP diagnostics and rename support.

use tower_lsp::lsp_types::{
    ClientCapabilities, CreateFilesParams, DeleteFilesParams, DidChangeWatchedFilesParams,
    MessageType, RenameFilesParams, WorkspaceEdit,
};
#[cfg(feature = "native")]
use tower_lsp::lsp_types::{FileChangeType, FileEvent, Url};

use super::{MaestroServer, ServerState};
use crate::ide::FileRenameService;

#[cfg(feature = "native")]
mod dependents;
#[cfg(feature = "native")]
use dependents::{
    affected_vue_source_paths, forget_corsa_vue_files, include_open_typecheck_documents,
    invalidate_corsa_disk_state, versioned_open_typecheck_dependents,
};

#[cfg(feature = "native")]
use tower_lsp::lsp_types::{
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern, Registration,
};

pub(super) fn record_watcher_support(state: &ServerState, capabilities: &ClientCapabilities) {
    #[cfg(feature = "native")]
    state.set_global_component_watcher_supported(
        capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files.as_ref())
            .and_then(|watched| watched.dynamic_registration)
            .unwrap_or(false),
    );
    #[cfg(not(feature = "native"))]
    let _ = (state, capabilities);
}

pub(super) async fn initialized(server: &MaestroServer) {
    register_typecheck_dependency_watcher(server).await;
    server
        .client
        .log_message(MessageType::INFO, "vize_maestro LSP server initialized")
        .await;
}

async fn register_typecheck_dependency_watcher(server: &MaestroServer) {
    #[cfg(feature = "native")]
    {
        if !server.state.is_lsp_typecheck_enabled()
            || !server.state.global_component_watcher_supported()
        {
            return;
        }
        if let Err(error) = server
            .client
            .register_capability(vec![typecheck_dependency_watcher_registration()])
            .await
        {
            tracing::warn!("failed to register typecheck dependency watcher: {error}");
        }
    }
    #[cfg(not(feature = "native"))]
    let _ = server;
}

#[cfg(feature = "native")]
fn typecheck_dependency_watcher_registration() -> Registration {
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: [
            "**/*.d.{ts,mts,cts}",
            "**/*.vue",
            "**/package.json",
            "**/tsconfig*.json",
            "**/jsconfig.json",
        ]
        .into_iter()
        .map(|pattern| FileSystemWatcher {
            glob_pattern: GlobPattern::String(pattern.into()),
            kind: None,
        })
        .collect(),
    };
    Registration {
        id: "vize-typecheck-dependencies".into(),
        method: "workspace/didChangeWatchedFiles".into(),
        register_options: serde_json::to_value(options).ok(),
    }
}

pub(super) async fn did_change_watched_files(
    server: &MaestroServer,
    params: &DidChangeWatchedFilesParams,
) {
    #[cfg(feature = "native")]
    {
        let changes = user_watched_file_events(&params.changes);
        if changes.is_empty() {
            return;
        }
        if changes_invalidate_disk_project_state(&server.state, &changes) {
            invalidate_corsa_disk_state(&server.state);
        }
        // Any watched change can affect an open importer; declaration changes
        // additionally invalidate the discoverable global-component cache.
        let global_components_invalidated = server.state.invalidate_global_component_references(
            changes.iter().map(|change| change.uri.as_str()),
        );
        let mut dependents = versioned_open_typecheck_dependents(
            &server.state,
            changes.iter().map(|change| change.uri.as_str()),
        );
        let deleted_paths = affected_vue_source_paths(
            &server.state,
            changes
                .iter()
                .filter(|change| change.typ == FileChangeType::DELETED)
                .map(|change| change.uri.as_str()),
        );
        if !deleted_paths.is_empty() {
            dependents = include_open_typecheck_documents(&server.state, dependents);
        }
        if dependents.is_empty() && !global_components_invalidated && deleted_paths.is_empty() {
            return;
        }
        server.state.invalidate_batch_cache();
        forget_corsa_vue_files(&server.state, &deleted_paths).await;
        publish_versioned_dependents(server, dependents).await;
    }
    #[cfg(not(feature = "native"))]
    let _ = (server, params);
}

#[cfg(feature = "native")]
fn user_watched_file_events(changes: &[FileEvent]) -> Vec<FileEvent> {
    changes
        .iter()
        .filter(|change| !is_internal_corsa_overlay_uri(&change.uri))
        .cloned()
        .collect()
}

#[cfg(feature = "native")]
fn is_internal_corsa_overlay_uri(uri: &Url) -> bool {
    let path = uri.path();
    path.contains("/node_modules/.vize/corsa-overlay/")
        || path.ends_with("/node_modules/.vize/corsa-overlay")
}

/// Whether watched changes moved project state the type checker only sees on disk.
///
/// Editing an open `.vue` source reaches the checker through its synchronized
/// virtual document, so treating those edits as disk changes would retire the
/// reusable editor session on every save. Changed closed `.vue` files are only
/// visible on disk and must invalidate cached project state just like
/// declaration, manifest, and configuration changes.
#[cfg(feature = "native")]
fn changes_invalidate_disk_project_state(state: &ServerState, changes: &[FileEvent]) -> bool {
    changes.iter().any(|change| {
        change.typ != FileChangeType::CHANGED
            || !change.uri.as_str().ends_with(".vue")
            || state.documents.version(&change.uri).is_none()
    })
}

#[cfg(feature = "native")]
pub(super) async fn invalidate_changed_document_disk_project_state(
    server: &MaestroServer,
    uri: &tower_lsp::lsp_types::Url,
) {
    if changes_invalidate_disk_project_state(
        &server.state,
        &[FileEvent {
            uri: uri.clone(),
            typ: FileChangeType::CHANGED,
        }],
    ) {
        invalidate_corsa_disk_state(&server.state);
    }
}

pub(super) async fn did_create_files(server: &MaestroServer, params: &CreateFilesParams) {
    #[cfg(feature = "native")]
    {
        let dependents = versioned_open_typecheck_dependents(
            &server.state,
            params.files.iter().map(|file| file.uri.as_str()),
        );
        record_created_files(&server.state, params);
        invalidate_corsa_disk_state(&server.state);
        publish_versioned_dependents(server, dependents).await;
    }
    #[cfg(not(feature = "native"))]
    let _ = (server, params);
}

#[cfg(any(test, feature = "native"))]
fn record_created_files(state: &ServerState, params: &CreateFilesParams) {
    #[cfg(feature = "native")]
    {
        state.invalidate_global_component_references(
            params.files.iter().map(|file| file.uri.as_str()),
        );
        for file in &params.files {
            state.track_workspace_vue_files(file.uri.as_str());
        }
        state.invalidate_batch_cache();
    }
    #[cfg(not(feature = "native"))]
    let _ = (state, params);
}

pub(super) async fn did_delete_files(server: &MaestroServer, params: &DeleteFilesParams) {
    #[cfg(feature = "native")]
    {
        let mut dependents = versioned_open_typecheck_dependents(
            &server.state,
            params.files.iter().map(|file| file.uri.as_str()),
        );
        let deleted_paths = affected_vue_source_paths(
            &server.state,
            params.files.iter().map(|file| file.uri.as_str()),
        );
        if !deleted_paths.is_empty() {
            dependents = include_open_typecheck_documents(&server.state, dependents);
        }
        record_deleted_files(&server.state, params);
        forget_corsa_vue_files(&server.state, &deleted_paths).await;
        invalidate_corsa_disk_state(&server.state);
        publish_versioned_dependents(server, dependents).await;
    }
    #[cfg(not(feature = "native"))]
    let _ = (server, params);
}

#[cfg(any(test, feature = "native"))]
fn record_deleted_files(state: &ServerState, params: &DeleteFilesParams) {
    #[cfg(feature = "native")]
    {
        state.invalidate_global_component_references(
            params.files.iter().map(|file| file.uri.as_str()),
        );
        for file in &params.files {
            state.forget_workspace_vue_files(file.uri.as_str());
        }
        state.invalidate_batch_cache();
    }
    #[cfg(not(feature = "native"))]
    let _ = (state, params);
}

pub(super) async fn will_rename_files(
    state: &ServerState,
    params: &RenameFilesParams,
) -> Option<WorkspaceEdit> {
    if !state.lsp_features().file_rename {
        return None;
    }
    FileRenameService::will_rename_files(state, params).await
}

pub(super) async fn did_rename_files(server: &MaestroServer, params: &RenameFilesParams) {
    #[cfg(feature = "native")]
    {
        let dependents = versioned_open_typecheck_dependents(
            &server.state,
            params
                .files
                .iter()
                .flat_map(|file| [file.old_uri.as_str(), file.new_uri.as_str()]),
        );
        let renamed_paths = affected_vue_source_paths(
            &server.state,
            params.files.iter().map(|file| file.old_uri.as_str()),
        );
        server.state.invalidate_global_component_references(
            params
                .files
                .iter()
                .flat_map(|file| [file.old_uri.as_str(), file.new_uri.as_str()]),
        );
        for file in &params.files {
            server
                .state
                .forget_workspace_vue_files(file.old_uri.as_str());
            server
                .state
                .track_workspace_vue_files(file.new_uri.as_str());
        }
        server.state.invalidate_batch_cache();
        forget_corsa_vue_files(&server.state, &renamed_paths).await;
        invalidate_corsa_disk_state(&server.state);
        publish_versioned_dependents(server, dependents).await;
    }
    if !server.state.lsp_features().file_rename {
        return;
    }

    let renamed = FileRenameService::did_rename_files(&server.state, params).await;
    for (old_uri, new_uri) in renamed {
        server
            .client
            .publish_diagnostics(old_uri, vec![], None)
            .await;
        server.publish_diagnostics(&new_uri).await;
    }
}

#[cfg(feature = "native")]
async fn publish_versioned_dependents(
    server: &MaestroServer,
    dependents: Vec<(tower_lsp::lsp_types::Url, i32)>,
) {
    for (dependent, version) in dependents {
        server
            .publish_diagnostics_if_version(&dependent, version)
            .await;
    }
}

#[cfg(all(test, feature = "native"))]
#[path = "workspace_files_tests.rs"]
mod tests;
