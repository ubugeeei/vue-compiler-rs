//! Restoring the authored `.vue` specifier in a diagnostic message.
//!
//! The import rewriter redirects an SFC import onto that file's generated
//! mirror module (`./Panel.vue` -> `./Panel.vue.ts`), so every checker message
//! that interpolates the module specifier quotes a path the author never wrote
//! while `vue-tsc` quotes the `.vue` spelling. That covers `TS2307`
//! (`Cannot find module ...`, #3397) but also `TS2305`/`TS2614`
//! (`Module '"..."' has no exported member ...`, including its
//! `Did you mean to use 'import X from "..."' instead?` suggestion, which names
//! a path the user cannot type), `TS2459`, and anything else that prints a
//! module symbol (#3438).
//!
//! Rewriting is therefore driven by the message text rather than by one
//! diagnostic code, and is gated on proof that the author did not write the
//! mirror spelling themselves.

use std::path::Path;

use super::{DiagnosticMapper, OriginalPosition};
use crate::batch::restore_virtual_vue_specifiers;
use crate::batch::virtual_specifier_message::{
    MISSING_VUE_IMPORT_SENTINEL, QUOTE_PAIRS, quoted_specifiers,
};
use crate::corsa_client::LspDiagnostic;
use vize_carton::{String, cstr};

/// The authored `.vue` spelling behind a generated mirror-module specifier, or
/// `None` when `specifier` is not one. `./Panel.vue.ts` is what the import
/// rewriter writes for an authored `./Panel.vue`; `./Panel.vue.tsx` is the TSX
/// SFC form. Anything else — including an authored `./util.ts` — is left alone.
pub(crate) fn mirror_module_specifier_source(specifier: &str) -> Option<&str> {
    specifier
        .strip_suffix(".ts")
        .or_else(|| specifier.strip_suffix(".tsx"))
        .filter(|source| source.ends_with(".vue"))
}

/// Every distinct mirror-module specifier quoted in `message`.
///
/// A checker message quotes a module specifier either directly
/// (`Cannot find module './Panel.vue.ts'`) or doubly, as a module symbol name
/// inside a quoted sentence (`Module '"./Panel.vue.ts"' ...`,
/// `... 'import Panel from "./Panel.vue.ts"' ...`), both of which the shared
/// [`quoted_specifiers`] scan covers.
fn quoted_mirror_specifiers(message: &str) -> Vec<&str> {
    quoted_specifiers(message)
        .into_iter()
        .filter(|candidate| mirror_module_specifier_source(candidate).is_some())
        .collect()
}

impl DiagnosticMapper<'_> {
    pub(crate) fn map_diagnostic_position_to_original(
        &mut self,
        virtual_path: &Path,
        diagnostic: &LspDiagnostic,
        code: Option<u32>,
    ) -> Option<OriginalPosition> {
        self.map_to_original(
            virtual_path,
            diagnostic.range.start.line,
            diagnostic.range.start.character,
        )
        .or_else(|| {
            (code == Some(2307)).then(|| {
                self.missing_vue_import_position(virtual_path, diagnostic.message.as_str())
            })?
        })
    }

    fn missing_vue_import_position(
        &mut self,
        virtual_path: &Path,
        message: &str,
    ) -> Option<OriginalPosition> {
        let authored = missing_vue_import_specifier_source(message)?;
        let virtual_file = self.project.find_by_diagnostic_virtual(virtual_path)?;
        let original_path = virtual_file.original_path.clone();
        let source = self.original_source(&original_path)?;
        let offset = authored_specifier_literal_offset(&source.content, authored)
            .or_else(|| source.content.find(authored))?;
        let (line, column) = source
            .line_index
            .offset_to_line_col(&source.content, offset as u32)?;

        Some(OriginalPosition {
            path: original_path,
            line,
            column,
            block_type: None,
        })
    }

    /// Rewrite every generated mirror-module specifier in `message` back to the
    /// spelling the author wrote.
    ///
    /// Replacement is quote-delimited rather than a bare substring swap: a
    /// specifier can be a suffix of a longer one (`./A.vue.ts` inside
    /// `../A.vue.ts`), and only the run that was actually quoted may change.
    pub(crate) fn devirtualized_module_message(
        &mut self,
        original: &OriginalPosition,
        message: String,
    ) -> String {
        let mut rewritten = if let Some(source) = self.original_source(&original.path) {
            restore_virtual_vue_specifiers(&message, &source.content)
        } else {
            message
        };
        let rewrites: Vec<(String, String)> = quoted_mirror_specifiers(&rewritten)
            .into_iter()
            .filter_map(|reported| {
                let authored = mirror_module_specifier_source(reported)?;
                self.author_wrote_the_vue_spelling(original, reported, authored)
                    .then(|| (String::from(reported), String::from(authored)))
            })
            .collect();
        for (reported, authored) in rewrites {
            for (open, close) in QUOTE_PAIRS {
                let quoted = cstr!("{open}{reported}{close}");
                if !rewritten.contains(quoted.as_str()) {
                    continue;
                }
                rewritten = rewritten
                    .replace(quoted.as_str(), cstr!("{open}{authored}{close}").as_str())
                    .into();
            }
        }
        rewritten
    }

    /// Whether the mirror spelling `reported` is provably the rewriter's and not
    /// the author's, so replacing it with `authored` cannot misquote the source.
    ///
    /// Two independent proofs, either of which suffices:
    ///
    /// 1. The authored bytes at the diagnostic position are the `authored`
    ///    string literal. An unresolved import anchors its diagnostic at the
    ///    specifier, so this reads exactly the import being reported (#3397).
    /// 2. The authored file contains the `reported` text nowhere at all. Most
    ///    messages that embed a specifier anchor somewhere else entirely — a
    ///    `TS2614` anchors at the imported member name — so there is no literal
    ///    to read at the position; a spelling absent from the whole file cannot
    ///    have come from it.
    ///
    /// A hand-written `import x from "./Panel.vue.ts"` fails both and keeps
    /// reporting exactly what it says.
    fn author_wrote_the_vue_spelling(
        &mut self,
        original: &OriginalPosition,
        reported: &str,
        authored: &str,
    ) -> bool {
        let Some(source) = self.original_source(&original.path) else {
            return false;
        };
        let content = &source.content;
        let index = &source.line_index;
        let literal_at_position = index
            .line_col_to_offset(content, original.line, original.column)
            .and_then(|offset| content.get(offset as usize..))
            .and_then(string_literal_at);
        literal_at_position == Some(authored) || !content.contains(reported)
    }
}

fn missing_vue_import_specifier_source(message: &str) -> Option<&str> {
    quoted_specifiers(message).into_iter().find_map(|reported| {
        reported
            .strip_suffix(MISSING_VUE_IMPORT_SENTINEL)
            .and_then(mirror_module_specifier_source)
    })
}

fn authored_specifier_literal_offset(source: &str, authored: &str) -> Option<usize> {
    ['\'', '"', '`'].into_iter().find_map(|quote| {
        let quoted = cstr!("{quote}{authored}{quote}");
        source.find(quoted.as_str()).map(|offset| offset + 1)
    })
}

/// The contents of the string literal starting at the beginning of `rest`, or
/// `None` when `rest` does not start with a quote.
fn string_literal_at(rest: &str) -> Option<&str> {
    let quote = rest
        .chars()
        .next()
        .filter(|character| matches!(character, '\'' | '"' | '`'))?;
    rest[quote.len_utf8()..]
        .split_once(quote)
        .map(|(literal, _)| literal)
}

#[cfg(test)]
mod tests {
    use super::super::{LineIndex, map_batch_diagnostics};
    use super::{mirror_module_specifier_source, quoted_mirror_specifiers, string_literal_at};
    use crate::batch::VirtualProject;
    use crate::batch::virtual_specifier_message::MISSING_VUE_IMPORT_SENTINEL;
    use crate::corsa_client::{LspDiagnostic, LspPosition, LspRange};
    use serde_json::json;
    use tempfile::TempDir;
    use vize_carton::cstr;

    #[test]
    fn only_generated_mirror_specifiers_map_back_to_an_authored_spelling() {
        assert_eq!(
            mirror_module_specifier_source("./Panel.vue.ts"),
            Some("./Panel.vue")
        );
        assert_eq!(
            mirror_module_specifier_source("../widgets/Panel.vue.tsx"),
            Some("../widgets/Panel.vue")
        );
        // An authored TypeScript specifier is not a mirror module, so its
        // message must keep the spelling the author wrote.
        assert_eq!(mirror_module_specifier_source("./util.ts"), None);
        assert_eq!(mirror_module_specifier_source("./Panel.vue"), None);
        assert_eq!(mirror_module_specifier_source("./types.d.ts"), None);
        assert_eq!(mirror_module_specifier_source("vue-router"), None);
    }

    #[test]
    fn mirror_specifiers_are_found_however_deeply_the_message_quotes_them() {
        assert_eq!(
            quoted_mirror_specifiers(
                "Cannot find module './Absent.vue.ts' or its corresponding type declarations."
            ),
            vec!["./Absent.vue.ts"]
        );
        // TS2614 quotes the module symbol inside single quotes and repeats the
        // specifier inside the suggestion, which is itself single-quoted.
        assert_eq!(
            quoted_mirror_specifiers(
                "Module '\"../components/Local.vue.ts\"' has no exported member 'Bare'. \
                 Did you mean to use 'import Bare from \"../components/Local.vue.ts\"' instead?"
            ),
            vec!["../components/Local.vue.ts"]
        );
        // Two different mirror modules in one message stay distinct.
        assert_eq!(
            quoted_mirror_specifiers(
                "Module '\"./A.vue.ts\"' declares 'x' locally, but it is exported from './B.vue.tsx'."
            ),
            vec!["./B.vue.tsx", "./A.vue.ts"]
        );
        // Nothing to rewrite, including a specifier that is not a mirror module
        // and prose that merely ends in the mirror suffix.
        assert_eq!(
            quoted_mirror_specifiers("Cannot find module './util.ts' or its type declarations."),
            Vec::<&str>::new()
        );
        assert_eq!(
            quoted_mirror_specifiers("Did you mean 'the file ./A.vue.ts'?"),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn reads_the_string_literal_at_a_diagnostic_position() {
        assert_eq!(string_literal_at("\"./Panel.vue\";\n"), Some("./Panel.vue"));
        assert_eq!(string_literal_at("'./Panel.vue'\n"), Some("./Panel.vue"));
        assert_eq!(string_literal_at("Panel from './x'"), None);
        assert_eq!(string_literal_at("\"unterminated"), None);
    }

    #[test]
    fn missing_vue_import_sentinel_reports_the_authored_specifier() {
        assert_eq!(
            super::missing_vue_import_specifier_source(
                "Cannot find module './Missing.vue.ts/__vize_missing_vue_import__'."
            ),
            Some("./Missing.vue")
        );
    }

    /// End to end through the collection point: a `TS2307` for an SFC import
    /// reaches the user quoting the authored `.vue` specifier, not the mirror
    /// module the rewriter redirected it onto (#3397).
    #[test]
    fn maps_missing_vue_ts2307_back_to_source_file() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().canonicalize().unwrap();
        let src_dir = project_root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let app_path = src_dir.join("App.vue");
        std::fs::write(
            &app_path,
            r#"<script setup lang="ts">
import MissingPanel from './MissingPanel.vue'
</script>

<template>
  <MissingPanel />
</template>
"#,
        )
        .unwrap();

        let mut project = VirtualProject::new(&project_root).unwrap();
        project.register_path(&app_path).unwrap();
        let virtual_file = project.find_by_original(&app_path).unwrap();
        let virtual_source = virtual_file.content.as_str();
        let reported = cstr!("./MissingPanel.vue.ts{MISSING_VUE_IMPORT_SENTINEL}");
        let offset = virtual_source
            .find(reported.as_str())
            .expect("the rewriter should redirect the import onto the mirror module");
        let (line, character) = LineIndex::new(virtual_source)
            .offset_to_line_col(virtual_source, offset as u32)
            .expect("virtual offset should map to LSP position");

        let diagnostics = map_batch_diagnostics(
            vec![(
                cstr!("file://{}", virtual_file.virtual_path.display()),
                vec![LspDiagnostic {
                    range: LspRange {
                        start: LspPosition { line, character },
                        end: LspPosition {
                            line,
                            character: character + 1,
                        },
                    },
                    severity: Some(1),
                    code: Some(json!("TS2307")),
                    source: Some("ts".into()),
                    message: cstr!(
                        "Cannot find module '{reported}' or its corresponding type declarations."
                    ),
                }],
            )],
            &project,
        );

        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.file, app_path);
        assert_eq!(diagnostic.code, Some(2307));
        assert_eq!(diagnostic.line, 1);
        assert_eq!(
            diagnostic.message,
            "Cannot find module './MissingPanel.vue' or its corresponding type declarations."
        );
    }
}
