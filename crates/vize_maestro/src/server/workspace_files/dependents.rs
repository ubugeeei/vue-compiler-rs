//! Open-importer refresh and Corsa overlay eviction for workspace file events.

use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::Url;

use super::super::{ServerState, importers};

pub(super) fn versioned_open_typecheck_dependents<'a>(
    state: &ServerState,
    uris: impl Iterator<Item = &'a str>,
) -> Vec<(Url, i32)> {
    let mut dependents = uris
        .filter_map(|uri| Url::parse(uri).ok())
        .flat_map(|uri| importers::open_typecheck_dependents(state, &uri))
        .collect::<Vec<_>>();
    dependents.sort();
    dependents.dedup();
    // A package.json event can retarget a bare package import. Rebuild the
    // reverse index from the unchanged open buffer after resolving the event
    // through the old manifest entry, so subsequent events address the new
    // source path without waiting for the user to type in the importer.
    for dependent in &dependents {
        if let Some(content) = state.documents.text(dependent) {
            state.open_imports.update(dependent, &content);
        }
    }
    dependents
        .into_iter()
        .filter_map(|uri| state.documents.version(&uri).map(|version| (uri, version)))
        .collect()
}

pub(super) fn include_open_typecheck_documents(
    state: &ServerState,
    mut documents: Vec<(Url, i32)>,
) -> Vec<(Url, i32)> {
    if !state.is_lsp_typecheck_enabled() {
        return documents;
    }
    documents.extend(state.documents.iter().filter_map(|document| {
        let uri = document.key();
        (uri.path().ends_with(".vue")
            || (state.jsx_typecheck_enabled() && crate::utils::is_jsx_path(uri.path())))
        .then(|| (uri.clone(), document.value().version))
    }));
    documents.sort();
    documents.dedup();
    documents
}

pub(super) fn affected_vue_source_paths<'a>(
    state: &ServerState,
    uris: impl Iterator<Item = &'a str>,
) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    for path in uris.filter_map(file_path) {
        if is_vue_file(&path) {
            sources.push(path.clone());
        }
        sources.extend(tracked_vue_descendant_paths(state, &path));
        sources.extend(
            importers::indexed_dependency_paths(state, &path)
                .into_iter()
                .filter(|dependency| is_vue_file(dependency)),
        );
    }
    sources.sort();
    sources.dedup();
    sources
}

fn tracked_vue_descendant_paths(state: &ServerState, root: &Path) -> Vec<PathBuf> {
    state
        .workspace_vue_file_uris()
        .into_iter()
        .filter_map(|uri| uri.to_file_path().ok())
        .filter(|path| path.starts_with(root) && is_vue_file(path))
        .collect()
}

/// Mark the on-disk project view cached by the reusable Corsa editor session stale.
///
/// The session keeps one long-lived type-checker process, so files it read from
/// disk while building its program stay pinned until the session is retired.
/// Declaration edits and file create, delete, or rename events change that view
/// without touching any synchronized virtual document, so the change is only
/// visible to the next Corsa-using request once the cached view is dropped.
///
/// The file-operation notification itself must stay non-blocking for
/// Corsa-free editor requests. In particular, the authored fixture asks
/// `workspace/symbol` immediately after `workspace/didCreateFiles`; that
/// request only needs the tracked file URI, not a TypeScript backend flush.
pub(super) fn invalidate_corsa_disk_state(state: &ServerState) {
    state.mark_corsa_disk_state_dirty();
}

pub(super) async fn forget_corsa_vue_files(state: &ServerState, deleted: &[PathBuf]) {
    if deleted.is_empty() {
        return;
    }
    if !state.has_corsa_bridge() {
        return;
    }
    let Some(bridge) = state.get_corsa_bridge().await else {
        return;
    };
    if let Err(error) = bridge.forget_vue_virtual_documents(deleted).await {
        tracing::warn!("failed to forget deleted Corsa Vue documents: {error}");
        state.retire_corsa_bridge(&bridge);
        return;
    }
    // Deleted Vue sources can leave generated files inside Corsa's private
    // materialized mirror. Retire the bridge so the next diagnostics pass
    // rebuilds that mirror from the current filesystem instead of resolving a
    // stale dependency.
    state.retire_corsa_bridge(&bridge);
}

fn file_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

fn is_vue_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "vue")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_methods)]

    use tower_lsp::lsp_types::Url;
    use vize_s0::cstr;

    use super::{
        ServerState, affected_vue_source_paths, include_open_typecheck_documents,
        versioned_open_typecheck_dependents,
    };

    #[test]
    fn package_manifest_event_reindexes_the_retargeted_vue_source() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("app");
        let package = root.path().join("packages/ui");
        let parent = app.join("src/Parent.vue");
        let original = package.join("src/Widget.vue");
        let renamed = package.join("src/Renamed.vue");
        std::fs::create_dir_all(parent.parent().unwrap()).unwrap();
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::write(&parent, "<template />\n").unwrap();
        std::fs::write(&original, "<template />\n").unwrap();
        write_manifest(&package, "Widget.vue");
        link_package(&package, &app.join("node_modules/@scope/ui"));
        let parent_uri = Url::from_file_path(&parent).unwrap();
        let source = "<script setup>import Widget from '@scope/ui/widget'; void Widget</script>";
        let state = ServerState::new();
        state
            .documents
            .open(parent_uri.clone(), source.to_owned(), 1, "vue".to_owned());
        state.update_virtual_docs(&parent_uri, source);

        std::fs::rename(&original, &renamed).unwrap();
        write_manifest(&package, "Renamed.vue");
        let manifest_uri = Url::from_file_path(package.join("package.json")).unwrap();
        assert_eq!(
            versioned_open_typecheck_dependents(&state, [manifest_uri.as_str()].into_iter()),
            [(parent_uri.clone(), 1)]
        );
        let renamed_uri = Url::from_file_path(renamed.canonicalize().unwrap()).unwrap();
        assert_eq!(
            versioned_open_typecheck_dependents(&state, [renamed_uri.as_str()].into_iter()),
            [(parent_uri, 1)],
            "the manifest refresh must index the new physical target",
        );
    }

    #[test]
    fn directory_events_evict_tracked_vue_descendants_without_disk_scan() {
        let root = tempfile::tempdir().unwrap();
        let components = root.path().join("components");
        let child = components.join("nested/Child.vue");
        let sibling_dir = root.path().join("components-old");
        let sibling = sibling_dir.join("Sibling.vue");
        std::fs::create_dir_all(child.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&sibling_dir).unwrap();
        std::fs::write(&child, "<template />\n").unwrap();
        std::fs::write(&sibling, "<template />\n").unwrap();
        let components_uri = Url::from_file_path(&components).unwrap();
        let sibling_uri = Url::from_file_path(&sibling_dir).unwrap();
        let state = ServerState::new();

        assert!(state.track_workspace_vue_files(components_uri.as_str()));
        assert!(state.track_workspace_vue_files(sibling_uri.as_str()));

        std::fs::rename(&components, root.path().join("renamed-components")).unwrap();

        assert_eq!(
            affected_vue_source_paths(&state, [components_uri.as_str()].into_iter()),
            [child],
            "directory rename/delete events must retire old tracked Vue virtual documents"
        );
    }

    #[test]
    fn open_typecheck_document_fallback_preserves_same_lsp_versions() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("Parent.vue");
        let child = root.path().join("Child.vue");
        std::fs::write(&parent, "<template />\n").unwrap();
        std::fs::write(&child, "<template />\n").unwrap();
        let parent_uri = Url::from_file_path(&parent).unwrap();
        let child_uri = Url::from_file_path(&child).unwrap();
        let state = ServerState::new();
        let source = "<script setup lang=\"ts\">import Child from './Child.vue'</script>";
        state
            .documents
            .open(parent_uri.clone(), source.to_owned(), 5, "vue".to_owned());

        assert_eq!(
            affected_vue_source_paths(&state, [child_uri.as_str()].into_iter()),
            [child],
        );
        assert_eq!(
            include_open_typecheck_documents(&state, Vec::new()),
            [(parent_uri, 5)],
            "delete events can re-publish open importers even when the reverse index missed the edge",
        );
    }

    fn write_manifest(package: &std::path::Path, target: &str) {
        std::fs::write(
            package.join("package.json"),
            cstr!("{{\"name\":\"@scope/ui\",\"exports\":{{\"./widget\":\"./src/{target}\"}}}}"),
        )
        .unwrap();
    }

    fn link_package(source: &std::path::Path, target: &std::path::Path) {
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(source, target).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(source, target).unwrap();
    }
}
