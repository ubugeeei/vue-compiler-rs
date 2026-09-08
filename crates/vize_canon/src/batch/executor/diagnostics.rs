use std::path::{Path, PathBuf};

use serde_json::Value;

use super::super::{Diagnostic, OriginalPosition, VirtualFile, VirtualProject};
use crate::corsa_client::LspDiagnostic;
use crate::file_uri::file_uri_to_path;
use vize_carton::{FxHashMap, String};

mod dedup;
mod keyof_indexed_assignment;
mod line_index;
mod module_resolution;
mod module_specifier;
mod skip_rules;
mod virtual_path_message;

pub(super) use dedup::dedup_diagnostics;
use line_index::LineIndex;
pub(super) use module_resolution::relative_module_resolves_on_disk;
pub(super) use skip_rules::{should_skip_diagnostic, should_skip_original_diagnostic};
use virtual_path_message::restore_authored_paths;
pub(super) use virtual_path_message::restore_authored_paths_in_messages;

pub(super) fn map_batch_diagnostics(
    results: Vec<(String, Vec<LspDiagnostic>)>,
    project: &VirtualProject,
) -> Vec<Diagnostic> {
    let diagnostic_count = results
        .iter()
        .fold(0usize, |acc, (_, diagnostics)| acc + diagnostics.len());
    let mut diagnostics = Vec::with_capacity(diagnostic_count);
    let mut mapper = DiagnosticMapper::new(project);

    for (uri, lsp_diagnostics) in results {
        let virtual_path = uri_to_path(uri.as_str());
        for diagnostic in lsp_diagnostics {
            if let Some(diagnostic) = mapper.map_lsp_diagnostic(&virtual_path, diagnostic) {
                diagnostics.push(diagnostic);
            }
        }
    }

    dedup_diagnostics(diagnostics)
}

pub(super) struct DiagnosticMapper<'a> {
    project: &'a VirtualProject,
    preserve_unused_diagnostics: bool,
    original_sources: FxHashMap<PathBuf, CachedSource>,
    virtual_line_indexes: FxHashMap<PathBuf, LineIndex>,
}

impl<'a> DiagnosticMapper<'a> {
    pub(super) fn new(project: &'a VirtualProject) -> Self {
        Self {
            project,
            preserve_unused_diagnostics: project.tsconfig_preserves_unused_diagnostics(),
            original_sources: FxHashMap::default(),
            virtual_line_indexes: FxHashMap::default(),
        }
    }

    pub(super) fn preserves_unused_diagnostics(&self) -> bool {
        self.preserve_unused_diagnostics
    }

    /// A checker message with every trace of the virtual project removed.
    ///
    /// Two independent halves of the mirroring have to be undone, and a message
    /// can carry both: the import rewriter's `./Panel.vue` -> `./Panel.vue.ts`
    /// specifier spelling, and the materialized root every path is printed from.
    fn authored_message(&mut self, original: &OriginalPosition, message: String) -> String {
        let message = self.devirtualized_module_message(original, message);
        restore_authored_paths(
            &message,
            self.project.virtual_root(),
            self.project.project_root(),
        )
    }

    fn map_lsp_diagnostic(
        &mut self,
        virtual_path: &Path,
        diagnostic: LspDiagnostic,
    ) -> Option<Diagnostic> {
        let code = parse_diagnostic_code(diagnostic.code.as_ref());
        if should_skip_diagnostic(code, &diagnostic.message) {
            return None;
        }
        if code == Some(6133) && !self.preserve_unused_diagnostics {
            return None;
        }
        if code == Some(2322)
            && self.is_keyof_indexed_assignment(
                virtual_path,
                diagnostic.range.start.line,
                diagnostic.range.start.character,
            )
        {
            return None;
        }

        let original = self.map_diagnostic_position_to_original(virtual_path, &diagnostic, code);
        if original
            .as_ref()
            .is_some_and(|original| should_skip_original_diagnostic(code, original))
        {
            return None;
        }

        // A `vize check src/App.vue` subset registers only the named files, so a
        // relative import of a sibling that exists on disk but sits outside the
        // subset (`import { x } from "./util"`) is reported as a false
        // `TS2307`. `tsc`/`vue-tsc` resolve it against the whole program;
        // suppress the false positive when the specifier resolves to a real
        // source file next to the importing file. Genuinely missing modules
        // (bad relative paths, bare packages) still surface.
        if code == Some(2307)
            && let Some(original) = original.as_ref()
            && relative_module_resolves_on_disk(&diagnostic.message, &original.path)
        {
            return None;
        }

        if let Some(original) = original {
            return Some(Diagnostic {
                message: self.authored_message(&original, diagnostic.message),
                line: original.line,
                column: original.column,
                file: original.path,
                code,
                severity: parse_severity(diagnostic.severity),
                block_type: original.block_type,
            });
        }

        None
    }

    /// Map a Corsa position to a virtual source-map origin or to an explicitly
    /// authored, in-place diagnostic path. The `checkJs` gate for JavaScript
    /// SFCs also lives here (#3322).
    pub(super) fn map_to_original(
        &mut self,
        virtual_path: &Path,
        line: u32,
        column: u32,
    ) -> Option<OriginalPosition> {
        if self.project.skips_typescript_diagnostics(virtual_path) {
            return None;
        }
        let Some(file) = self.project.find_by_diagnostic_virtual(virtual_path) else {
            return self.project.diagnostic_position(virtual_path, line, column);
        };
        let virtual_offset = self.virtual_offset(file, line, column)?;
        let (original_offset, _, block_type) =
            file.source_map.get_original_position(virtual_offset)?;
        let cached = self.original_source(&file.original_path)?;
        let (original_line, original_column) = cached
            .line_index
            .offset_to_line_col(&cached.content, original_offset)?;

        Some(OriginalPosition {
            path: file.original_path.clone(),
            line: original_line,
            column: original_column,
            block_type,
        })
    }

    fn virtual_offset(&mut self, file: &VirtualFile, line: u32, column: u32) -> Option<u32> {
        if !self.virtual_line_indexes.contains_key(&file.virtual_path) {
            self.virtual_line_indexes
                .insert(file.virtual_path.clone(), LineIndex::new(&file.content));
        }
        let line_index = self.virtual_line_indexes.get(&file.virtual_path)?;
        line_index.line_col_to_offset(&file.content, line, column)
    }

    fn original_source(&mut self, path: &Path) -> Option<&CachedSource> {
        if !self.original_sources.contains_key(path) {
            let content: String = std::fs::read_to_string(path).ok()?.into();
            let line_index = LineIndex::new(&content);
            self.original_sources.insert(
                path.to_path_buf(),
                CachedSource {
                    content,
                    line_index,
                },
            );
        }

        self.original_sources.get(path)
    }

    pub(super) fn is_keyof_indexed_assignment(
        &mut self,
        virtual_path: &Path,
        line: u32,
        column: u32,
    ) -> bool {
        let Some(file) = self.project.find_by_diagnostic_virtual(virtual_path) else {
            return false;
        };
        let Some(offset) = self.virtual_offset(file, line, column) else {
            return false;
        };
        keyof_indexed_assignment::matches_at(&file.content, &file.virtual_path, offset)
    }
}

struct CachedSource {
    content: String,
    line_index: LineIndex,
}

fn uri_to_path(uri: &str) -> PathBuf {
    file_uri_to_path(uri).unwrap_or_else(|| PathBuf::from(uri))
}

fn parse_diagnostic_code(code: Option<&Value>) -> Option<u32> {
    match code {
        Some(Value::Number(value)) => value.as_u64().and_then(|value| u32::try_from(value).ok()),
        Some(Value::String(value)) => value
            .strip_prefix("TS")
            .unwrap_or(value.as_str())
            .parse()
            .ok(),
        _ => None,
    }
}

fn parse_severity(severity: Option<i32>) -> u8 {
    match severity {
        Some(value) if (1..=4).contains(&value) => value as u8,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LineIndex, map_batch_diagnostics, parse_diagnostic_code, parse_severity,
        should_skip_diagnostic, should_skip_original_diagnostic, uri_to_path,
    };
    use crate::batch::{OriginalPosition, SfcBlockType, VirtualProject};
    use serde_json::json;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use vize_carton::cstr;

    /// Regression for the #1389 double-emission. An undefined name used in a
    /// template interpolation (`{{ missingThing }}`) is referenced twice in the
    /// generated virtual TS — once by the normal template-expression statement
    /// (`void (missingThing); // Interpolation`) and once by the dedicated
    /// "Undefined references from template" check (`void (missingThing);`).
    /// Both spans map back to the identical interpolation in the source, so
    /// Corsa reports the same `TS2304` at two virtual positions and the error
    /// surfaced for the user exactly twice. The collection point must collapse
    /// those exact duplicates to one diagnostic. (Verified empirically against
    /// the native TypeScript 7 CLI.)
    #[test]
    fn duplicated_template_diagnostic_is_reported_once() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().canonicalize().unwrap();
        let src_dir = project_root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let app_path = src_dir.join("App.vue");
        std::fs::write(
            &app_path,
            "<script setup lang=\"ts\">\n</script>\n<template><div>{{ missingThing }}</div></template>\n",
        )
        .unwrap();

        let mut project = VirtualProject::new(&project_root).unwrap();
        project.register_path(&app_path).unwrap();
        let virtual_file = project.find_by_original(&app_path).unwrap();
        let virtual_source = virtual_file.content.as_str();

        // `missingThing` is emitted at two distinct virtual positions that both
        // map to the same source interpolation. Build one LSP diagnostic per
        // occurrence, mirroring what Corsa returns.
        let message = "Cannot find name 'missingThing'.";
        let diagnostics: Vec<_> = virtual_source
            .match_indices("void (missingThing)")
            .map(|(at, _)| at + "void (".len())
            .map(|offset| {
                let (line, character) = LineIndex::new(virtual_source)
                    .offset_to_line_col(virtual_source, offset as u32)
                    .expect("virtual offset should map to LSP position");
                crate::corsa_client::LspDiagnostic {
                    range: crate::corsa_client::LspRange {
                        start: crate::corsa_client::LspPosition { line, character },
                        end: crate::corsa_client::LspPosition {
                            line,
                            character: character + "missingThing".len() as u32,
                        },
                    },
                    severity: Some(1),
                    code: Some(json!("TS2304")),
                    source: Some("ts".into()),
                    message: message.into(),
                }
            })
            .collect();

        assert_eq!(
            diagnostics.len(),
            2,
            "expected `missingThing` to be generated at two virtual positions"
        );

        let mapped = map_batch_diagnostics(
            vec![(file_uri_for(&virtual_file.virtual_path), diagnostics)],
            &project,
        );

        assert_eq!(
            mapped.len(),
            1,
            "the duplicated template diagnostic must be deduplicated: {mapped:#?}"
        );
        assert_eq!(mapped[0].file, app_path);
        assert_eq!(mapped[0].code, Some(2304));
    }

    #[test]
    fn parses_numeric_and_string_diagnostic_codes() {
        assert_eq!(parse_diagnostic_code(Some(&json!(2322))), Some(2322));
        assert_eq!(parse_diagnostic_code(Some(&json!("TS2304"))), Some(2304));
        assert_eq!(parse_diagnostic_code(Some(&json!("2551"))), Some(2551));
        assert_eq!(parse_diagnostic_code(Some(&json!(false))), None);
    }

    #[test]
    fn normalizes_lsp_severity() {
        assert_eq!(parse_severity(Some(1)), 1);
        assert_eq!(parse_severity(Some(2)), 2);
        assert_eq!(parse_severity(Some(9)), 1);
        assert_eq!(parse_severity(None), 1);
    }

    #[test]
    fn strips_file_uri_scheme() {
        assert_eq!(
            uri_to_path("file:///workspace/src/App.vue.ts"),
            PathBuf::from("/workspace/src/App.vue.ts")
        );
    }

    #[test]
    fn decodes_file_uri_path_bytes() {
        assert_eq!(
            uri_to_path("file:///workspace/pages/%5Bname%5D%20%231.vue.ts"),
            PathBuf::from("/workspace/pages/[name] #1.vue.ts")
        );
    }

    #[test]
    fn ts2307_module_not_found_diagnostics_are_not_skipped_globally() {
        let vue_msg = "Cannot find module './app.vue' or its corresponding type declarations.";
        let vue_ts_msg =
            "Cannot find module './app.vue.ts' or its corresponding type declarations.";
        let non_vue_msg = "Cannot find module 'lodash-es' or its corresponding type declarations.";

        assert!(!should_skip_diagnostic(Some(2307), vue_msg));
        assert!(!should_skip_diagnostic(Some(2307), vue_ts_msg));
        assert!(!should_skip_diagnostic(Some(2307), non_vue_msg));

        // TS6133 is location-sensitive: user-mapped diagnostics must survive,
        // while generated virtual-SFC helper diagnostics are handled after
        // source mapping by should_skip_original_diagnostic().
        assert!(!should_skip_diagnostic(Some(6133), "any message"));
        assert!(should_skip_diagnostic(Some(2666), "any message"));
        // TS7006/7043/7044 (noImplicitAny family) must surface to match
        // vue-tsc — #966.
        assert!(!should_skip_diagnostic(Some(7006), "any message"));
        assert!(!should_skip_diagnostic(Some(7043), "any message"));
        assert!(!should_skip_diagnostic(Some(7044), "any message"));
        assert!(!should_skip_diagnostic(Some(2322), "any message"));
        assert!(should_skip_diagnostic(
            Some(2322),
            "Type 'ArrayBuffer | SharedArrayBuffer' is not assignable to type 'ArrayBuffer'."
        ));
        assert!(!should_skip_diagnostic(None, "any message"));
    }

    #[test]
    fn ts6133_suppression_is_limited_to_unmapped_vue_generated_code() {
        let unmapped_vue = OriginalPosition {
            path: PathBuf::from("App.vue"),
            line: 0,
            column: 0,
            block_type: None,
        };
        let mapped_vue = OriginalPosition {
            path: PathBuf::from("App.vue"),
            line: 1,
            column: 6,
            block_type: Some(SfcBlockType::ScriptSetup),
        };
        let plain_ts = OriginalPosition {
            path: PathBuf::from("main.ts"),
            line: 0,
            column: 6,
            block_type: None,
        };

        assert!(should_skip_original_diagnostic(Some(6133), &unmapped_vue));
        assert!(!should_skip_original_diagnostic(Some(6133), &mapped_vue));
        assert!(!should_skip_original_diagnostic(Some(6133), &plain_ts));
        assert!(!should_skip_original_diagnostic(Some(2322), &unmapped_vue));
    }

    #[test]
    fn suppresses_vue_ts2307_when_vue_sibling_exists_on_disk() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().canonicalize().unwrap();
        let src_dir = project_root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let app_path = src_dir.join("App.vue");
        std::fs::write(
            &app_path,
            r#"<script setup lang="ts">
import ExistingPanel from './ExistingPanel.vue'
</script>

<template>
  <ExistingPanel />
</template>
"#,
        )
        .unwrap();
        std::fs::write(
            src_dir.join("ExistingPanel.vue"),
            r#"<template><section /></template>
"#,
        )
        .unwrap();

        let mut project = VirtualProject::new(&project_root).unwrap();
        project.register_path(&app_path).unwrap();
        let virtual_file = project.find_by_original(&app_path).unwrap();
        let diagnostic = ts2307_diagnostic_at(
            virtual_file.content.as_str(),
            "ExistingPanel.vue.ts",
            "Cannot find module './ExistingPanel.vue.ts' or its corresponding type declarations.",
        );

        let diagnostics = map_batch_diagnostics(
            vec![(file_uri_for(&virtual_file.virtual_path), vec![diagnostic])],
            &project,
        );

        assert!(
            diagnostics.is_empty(),
            "existing Vue sibling false positive should stay suppressed: {diagnostics:#?}"
        );
    }

    #[test]
    fn maps_unmapped_diagnostics_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let project = VirtualProject::new(temp_dir.path()).unwrap();
        let diagnostics = map_batch_diagnostics(
            vec![(
                cstr!("file:///workspace/src/App.vue.ts"),
                vec![crate::corsa_client::LspDiagnostic {
                    range: crate::corsa_client::LspRange {
                        start: crate::corsa_client::LspPosition {
                            line: 3,
                            character: 5,
                        },
                        end: crate::corsa_client::LspPosition {
                            line: 3,
                            character: 12,
                        },
                    },
                    severity: Some(1),
                    code: Some(json!("TS2322")),
                    source: Some("ts".into()),
                    message: "Type 'string' is not assignable to type 'number'.".into(),
                }],
            )],
            &project,
        );

        insta::assert_debug_snapshot!("maps_unmapped_diagnostics_snapshot", diagnostics);
    }

    #[test]
    fn maps_user_ts6133_but_suppresses_generated_vue_ts6133() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("src").join("App.vue");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(
            temp_dir.path().join("tsconfig.json"),
            r#"{
  "compilerOptions": {
    "noUnusedLocals": true
  },
  "include": ["src/**/*"]
}"#,
        )
        .unwrap();
        std::fs::write(
            &source,
            r#"<script setup lang="ts">
const used = 1
const unusedLocal = 2
</script>

<template>{{ used }}</template>
"#,
        )
        .unwrap();

        let source = source.canonicalize().unwrap();
        let mut project = VirtualProject::new(temp_dir.path()).unwrap();
        project.register_path(&source).unwrap();
        let virtual_file = project.find_by_original(&source).unwrap();
        let virtual_uri = file_uri_for(&virtual_file.virtual_path);

        let diagnostics = map_batch_diagnostics(
            vec![(
                virtual_uri,
                vec![
                    ts6133_diagnostic_at(
                        virtual_file.content.as_str(),
                        "unusedLocal",
                        "'unusedLocal' is declared but its value is never read.",
                    ),
                    ts6133_diagnostic_at(
                        virtual_file.content.as_str(),
                        "const defineProps",
                        "'defineProps' is declared but its value is never read.",
                    ),
                ],
            )],
            &project,
        );

        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].file, source);
        assert_eq!(diagnostics[0].line, 2);
        assert_eq!(diagnostics[0].column, 6);
        assert_eq!(diagnostics[0].code, Some(6133));
        assert!(diagnostics[0].message.contains("unusedLocal"));
        assert_eq!(diagnostics[0].block_type, Some(SfcBlockType::ScriptSetup));
    }

    fn ts2307_diagnostic_at(
        virtual_source: &str,
        needle: &str,
        message: &str,
    ) -> crate::corsa_client::LspDiagnostic {
        let (line, character) = virtual_position_for(virtual_source, needle);
        crate::corsa_client::LspDiagnostic {
            range: crate::corsa_client::LspRange {
                start: crate::corsa_client::LspPosition { line, character },
                end: crate::corsa_client::LspPosition {
                    line,
                    character: character + 1,
                },
            },
            severity: Some(1),
            code: Some(json!("TS2307")),
            source: Some("ts".into()),
            message: message.into(),
        }
    }

    fn ts6133_diagnostic_at(
        virtual_source: &str,
        needle: &str,
        message: &str,
    ) -> crate::corsa_client::LspDiagnostic {
        let (line, character) = virtual_position_for(virtual_source, needle);
        crate::corsa_client::LspDiagnostic {
            range: crate::corsa_client::LspRange {
                start: crate::corsa_client::LspPosition { line, character },
                end: crate::corsa_client::LspPosition { line, character },
            },
            severity: Some(1),
            code: Some(json!("TS6133")),
            source: Some("ts".into()),
            message: message.into(),
        }
    }

    fn virtual_position_for(virtual_source: &str, needle: &str) -> (u32, u32) {
        let offset = virtual_source
            .find(needle)
            .unwrap_or_else(|| panic!("expected virtual source to contain {needle:?}"));
        LineIndex::new(virtual_source)
            .offset_to_line_col(virtual_source, offset as u32)
            .expect("virtual offset should map to LSP position")
    }

    fn file_uri_for(path: &std::path::Path) -> vize_carton::String {
        cstr!("file://{}", path.display())
    }
}
