//! Recovery for Corsa locations inside Canon's external mirror subtree.

use tower_lsp::lsp_types::{Location, Range, Url};
use vize_s0::cstr;

use crate::ide::IdeContext;
use crate::ide::diagnostics::VirtualTsResult;
use crate::ide::diagnostics::corsa::semantic_links_after_import_rewrite;

use super::canonical::{CanonicalVirtualDocument, map_lsp_range_to_source};

pub(super) fn map_location(
    ctx: &IdeContext<'_>,
    location: &vize_canon::LspLocation,
) -> Option<Location> {
    let parsed = Url::parse(&location.uri).ok()?;
    let path = parsed.to_file_path().ok()?;
    let file_name = path.file_name()?.to_str()?;
    let vue_file_name = file_name
        .strip_suffix(".tsx")
        .or_else(|| file_name.strip_suffix(".ts"))?;
    if !vue_file_name.ends_with(".vue") {
        return None;
    }

    let mirror_source_path = path.with_file_name(vue_file_name);
    // A neighboring authored `Component.vue.ts` is a valid real TypeScript
    // file, not proof that Corsa returned one of Canon's mirror documents.
    // Decode the explicit external-mirror identity first so unknown synthetic
    // `.vue.ts` locations continue to fail closed.
    let source_path = vize_canon::batch::external_mirror_original_path(&mirror_source_path)?;
    if !source_path.is_file() {
        return None;
    }

    let uri = Url::from_file_path(source_path).ok()?;
    if let Some(range) = map_range(ctx, &uri, &location.range) {
        return Some(Location { uri, range });
    }

    Some(Location {
        uri,
        range: Range {
            start: tower_lsp::lsp_types::Position {
                line: 0,
                character: 0,
            },
            end: tower_lsp::lsp_types::Position {
                line: 0,
                character: 0,
            },
        },
    })
}

fn map_range(
    ctx: &IdeContext<'_>,
    source_uri: &Url,
    range: &vize_canon::LspRange,
) -> Option<Range> {
    let source_path = source_uri.to_file_path().ok()?;
    let source = std::fs::read_to_string(&source_path).ok()?;
    let rewriter = vize_canon::ImportRewriter::new();
    let virtual_ts_options = ctx.state.virtual_ts_options();
    let generated = vize_canon::batch::generate_vue_document_virtual_ts_with_options(
        &source_path,
        &source,
        &virtual_ts_options,
        &rewriter,
        false,
        vize_canon::batch::VueDocumentVirtualTsOptions {
            options_api: ctx.state.options_api_enabled(),
            legacy_vue2: ctx.state.legacy_vue2_enabled(),
            preserve_event_navigation: true,
            dialect: ctx.state.type_checker_vue_version(),
            preserve_missing_vue_diagnostics: true,
        },
    )
    .ok()?;

    let mirror_doc = CanonicalVirtualDocument {
        source_uri: source_uri.clone(),
        request_uri: cstr!("{}{}", source_uri.path(), generated.virtual_suffix),
        virtual_result: VirtualTsResult {
            code: generated.code.to_string(),
            source_mappings: generated.mappings,
            semantic_links: semantic_links_after_import_rewrite(
                generated.semantic_links,
                &generated.import_source_map,
            ),
            import_source_map: generated.import_source_map,
            user_code_start_line: 0,
            sfc_script_start_line: 0,
            template_scope_start_line: 0,
            line_mappings: Vec::new(),
            skipped_import_lines: 0,
        },
        dependencies: Vec::new(),
        materialized_sources: Vec::new(),
        session_project_roots: Vec::new(),
    };
    map_lsp_range_to_source(&source, &mirror_doc, range)
}

#[cfg(test)]
mod tests {
    use std::path::{Component, PathBuf};

    use tower_lsp::lsp_types::Url;
    use vize_s0::cstr;

    use super::map_location;
    use crate::{ide::IdeContext, server::ServerState};

    #[test]
    #[cfg(not(windows))]
    fn external_mirror_locations_return_to_the_authored_vue_file() {
        let workspace = tempfile::tempdir().unwrap();
        let source = workspace.path().join("packages/ui/src/Widget.vue");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "<template />\n").unwrap();
        let source = source.canonicalize().unwrap();
        let mut mirror = workspace.path().join(".vize/__vize_external__");
        for component in source.components() {
            if let Component::Normal(part) = component {
                mirror.push(part);
            }
        }
        let mirror = PathBuf::from(cstr!("{}.ts", mirror.display()).as_str());

        let host = workspace.path().join("Host.vue");
        std::fs::write(&host, "<template />\n").unwrap();
        let host_uri = Url::from_file_path(&host).unwrap();
        let state = ServerState::new();
        state.documents.open(
            host_uri.clone(),
            "<template />\n".to_owned(),
            1,
            "vue".to_owned(),
        );
        let context = IdeContext::new(&state, &host_uri, 0).unwrap();
        let location = vize_canon::LspLocation {
            uri: Url::from_file_path(mirror).unwrap().to_string(),
            range: vize_canon::LspRange {
                start: vize_canon::LspPosition {
                    line: 0,
                    character: 0,
                },
                end: vize_canon::LspPosition {
                    line: 0,
                    character: 0,
                },
            },
        };

        let mapped = map_location(&context, &location).expect("mapped source location");
        assert_eq!(mapped.uri, Url::from_file_path(source).unwrap());
    }
}
