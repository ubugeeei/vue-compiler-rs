//! Tests for the diagnostics aggregation pipeline.
mod macro_scope;
mod self_closing_compatibility;
use std::fs;

use super::{DiagnosticBuilder, DiagnosticService, Severity, offset_to_line_col, sources};
use crate::server::ServerState;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Url};

#[derive(Debug)]
#[allow(dead_code)]
struct DiagnosticSnapshot {
    source: Option<String>,
    severity: Option<&'static str>,
    code: Option<String>,
    message: String,
    range: (u32, u32, u32, u32),
}

fn state_with_lsp_diagnostics(lint: bool, typecheck: bool) -> ServerState {
    let state = ServerState::new();
    state.apply_lsp_initialization_options(Some(&serde_json::json!({
        "lint": lint,
        "typecheck": typecheck
    })));
    state
}

fn state_with_ecosystem_diagnostics() -> ServerState {
    let state = ServerState::new();
    state.apply_lsp_initialization_options(Some(&serde_json::json!({
        "ecosystem": true
    })));
    state
}

fn diagnostic_snapshots(diagnostics: &[Diagnostic]) -> Vec<DiagnosticSnapshot> {
    diagnostics
        .iter()
        .map(|diagnostic| DiagnosticSnapshot {
            source: diagnostic.source.clone(),
            severity: severity_label(diagnostic.severity),
            code: diagnostic.code.as_ref().map(|code| match code {
                NumberOrString::Number(code) => code.to_string(),
                NumberOrString::String(code) => code.clone(),
            }),
            message: diagnostic.message.clone(),
            range: (
                diagnostic.range.start.line,
                diagnostic.range.start.character,
                diagnostic.range.end.line,
                diagnostic.range.end.character,
            ),
        })
        .collect()
}

fn severity_label(severity: Option<DiagnosticSeverity>) -> Option<&'static str> {
    let severity = severity?;
    if severity == DiagnosticSeverity::ERROR {
        Some("error")
    } else if severity == DiagnosticSeverity::WARNING {
        Some("warning")
    } else if severity == DiagnosticSeverity::INFORMATION {
        Some("information")
    } else if severity == DiagnosticSeverity::HINT {
        Some("hint")
    } else {
        Some("unknown")
    }
}

fn assert_diagnostics_snapshot(name: &str, diagnostics: &[Diagnostic]) {
    insta::with_settings!({
        snapshot_path => "snapshots"
    }, {
        insta::assert_debug_snapshot!(name, diagnostic_snapshots(diagnostics));
    });
}

#[test]
fn test_diagnostic_builder() {
    let diagnostic = DiagnosticBuilder::new("Test error")
        .severity(Severity::Warning)
        .source("test")
        .code(42)
        .build();

    assert_eq!(diagnostic.message, "Test error");
    assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(diagnostic.source, Some("test".to_string()));
    assert_eq!(diagnostic.code, Some(NumberOrString::Number(42)));
}

#[test]
fn test_severity_conversion() {
    assert_eq!(
        DiagnosticSeverity::from(Severity::Error),
        DiagnosticSeverity::ERROR
    );
    assert_eq!(
        DiagnosticSeverity::from(Severity::Warning),
        DiagnosticSeverity::WARNING
    );
    assert_eq!(
        DiagnosticSeverity::from(Severity::Information),
        DiagnosticSeverity::INFORMATION
    );
    assert_eq!(
        DiagnosticSeverity::from(Severity::Hint),
        DiagnosticSeverity::HINT
    );
}

#[test]
fn offset_to_line_col_counts_utf16_code_units() {
    let source = "const icon = \"😀\"; missing";
    let offset = source.find("missing").unwrap();

    assert_eq!(offset_to_line_col(source, offset), (0, 19));
}

#[test]
fn collect_short_circuits_dependent_diagnostics_after_sfc_parse_error() {
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///Broken.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<template><div></div>".to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);
    let diagnostic_sources: Vec<_> = diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.source.as_deref())
        .collect();

    assert_eq!(diagnostic_sources, vec![sources::SFC_PARSER]);
}

#[test]
fn collect_keeps_type_diagnostics_for_parseable_sfc() {
    let state = state_with_lsp_diagnostics(false, true);
    let uri = Url::parse("file:///Component.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup>const props = defineProps(['count'])</script><template>{{ props.count }}</template>".to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);
    let has_type_diagnostic = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.source.as_deref() == Some(sources::TYPE_CHECKER));

    assert_eq!(has_type_diagnostic, !cfg!(feature = "native"));
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.source.as_deref() == Some(sources::SFC_PARSER))
    );
}

#[test]
fn collect_reports_props_destructure_default_type_mismatch_for_lsp() {
    // Editor should surface the same DEFINE_PROPS_DESTRUCTURE_DEFAULT_TYPE
    // error that `vize check` shows, since TypeScript's checker cannot
    // detect a destructure default that conflicts with a Vue prop type.
    let state = state_with_lsp_diagnostics(false, false);
    let uri = Url::parse("file:///Bad.vue").unwrap();
    state.documents.open(
        uri.clone(),
        r#"<script setup lang="ts">
const { msg = 0 } = defineProps<{ msg?: string }>();
</script>

<template>
  <div>{{ msg }}</div>
</template>
"#
        .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.source.as_deref() == Some(sources::SFC_COMPILER))
        .expect("expected an SFC compile diagnostic");
    assert!(
        diagnostic
            .message
            .contains("DEFINE_PROPS_DESTRUCTURE_DEFAULT_TYPE"),
        "expected DEFINE_PROPS_DESTRUCTURE_DEFAULT_TYPE in message, got: {}",
        diagnostic.message
    );
    assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));

    // The diagnostic must point at the offending default value (`0` on the
    // second SFC line, after `const { msg = `), not at the start of the
    // `<script setup>` block (line 0), which was the previous behavior.
    assert_eq!(
        diagnostic.range.start,
        tower_lsp::lsp_types::Position {
            line: 1,
            character: 14,
        },
        "diagnostic should point at the default value `0`, got {:?}",
        diagnostic.range,
    );
    assert_eq!(
        diagnostic.range.end,
        tower_lsp::lsp_types::Position {
            line: 1,
            character: 15,
        },
        "diagnostic should span the single-character default `0`, got {:?}",
        diagnostic.range,
    );
}

#[test]
fn collect_does_not_report_sfc_compile_diagnostic_for_valid_default() {
    let state = state_with_lsp_diagnostics(false, false);
    let uri = Url::parse("file:///Good.vue").unwrap();
    state.documents.open(
        uri.clone(),
        r#"<script setup lang="ts">
const { msg = "ok" } = defineProps<{ msg?: string }>();
</script>

<template>
  <div>{{ msg }}</div>
</template>
"#
        .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.source.as_deref() != Some(sources::SFC_COMPILER)),
        "expected no SFC compile diagnostics, got: {:?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.source.as_deref(), diagnostic.message.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn collect_surfaces_sfc_level_lint_diagnostics() {
    let state = state_with_lsp_diagnostics(true, false);
    let uri = Url::parse("file:///OutOfOrder.vue").unwrap();
    let source = "<style scoped>.a {}</style>\n<script setup>const count = 1</script>";
    state
        .documents
        .open(uri.clone(), source.into(), 1, "vue".into());

    let diagnostics = DiagnosticService::collect(&state, &uri);
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.source.as_deref() == Some(sources::LINTER)
                && diagnostic.code
                    == Some(NumberOrString::String("vue/sfc-element-order".to_string()))
        })
        .expect("SFC-level lint diagnostic");

    assert_eq!(diagnostic.range.start.line, 1);
    let message = diagnostic.message.as_str();
    assert!(message.contains("<script setup> should come before"));
}

#[test]
fn collect_lints_standalone_html_with_configured_rule() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("vize.config.json"),
        r#"{
            "lsp": { "lint": true },
            "linter": {
                "rules": {
                    "script/no-options-api": "error"
                }
            }
        }"#,
    )
    .unwrap();

    let state = ServerState::new();
    state.load_lsp_config(dir.path());

    let source_path = dir.path().join("index.html");
    let uri = Url::from_file_path(&source_path).unwrap();
    let source = r##"<!doctype html>
<html>
<head>
  <script src="https://unpkg.com/vue@3/dist/vue.global.js"></script>
</head>
<body>
  <div id="app">{{ count }}</div>
  <script>
Vue.createApp({
  data() {
return { count: 0 }
  }
}).mount("#app")
  </script>
</body>
</html>
"##;
    state
        .documents
        .open(uri.clone(), source.to_string(), 1, "html".to_string());
    state.update_virtual_docs(&uri, source);

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(state.get_virtual_docs(&uri).is_some());
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.source.as_deref() != Some(sources::SFC_PARSER))
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.source.as_deref() == Some(sources::LINTER)
            && diagnostic.code == Some(NumberOrString::String("script/no-options-api".to_string()))
    }));
}

#[test]
fn collect_reports_unknown_file_route_params() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("src/pages/users/[id].vue");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    let source = r#"<script setup lang="ts">
import { useRoute } from "vue-router"
const route = useRoute()
route.params.slug
</script>"#;
    fs::write(&source_path, source).unwrap();

    let state = state_with_ecosystem_diagnostics();
    let uri = Url::from_file_path(&source_path).unwrap();
    state
        .documents
        .open(uri.clone(), source.to_string(), 1, "vue".to_string());

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.source.as_deref() == Some("vize/ecosystem")
            && diagnostic.code
                == Some(NumberOrString::String(
                    "ecosystem/vue-router-route-param".to_string(),
                ))
    }));
}

#[test]
fn collect_reports_missing_define_art_source_file() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("MissingSource.art.vue");
    let uri = Url::from_file_path(&source_path).unwrap();
    let source = r#"<script setup lang="ts">
defineArt("./Missing.vue", {
  title: "Missing",
});
</script>

<art>
  <variant name="Default">
<Missing />
  </variant>
</art>
"#;

    let state = state_with_lsp_diagnostics(true, false);
    state
        .documents
        .open(uri.clone(), source.to_string(), 1, "art-vue".to_string());

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.source.as_deref() == Some(sources::MUSEA)
            && diagnostic.code
                == Some(NumberOrString::String(
                    "musea/define-art-source-not-found".to_string(),
                ))
    }));
}

#[test]
fn collect_reports_unknown_route_path_params() {
    let source = r#"<script setup lang="ts">
import { useRoute } from "vue-router"
const route = useRoute()
route.params.slug
</script>
<route lang="json">
{ "path": "/users/:id" }
</route>"#;

    let state = state_with_ecosystem_diagnostics();
    let uri = Url::parse("file:///RoutePathParams.vue").unwrap();
    state
        .documents
        .open(uri.clone(), source.to_string(), 1, "vue".to_string());

    let diagnostics = DiagnosticService::collect(&state, &uri);
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.source.as_deref() == Some("vize/ecosystem")
                && diagnostic.code
                    == Some(NumberOrString::String(
                        "ecosystem/vue-router-route-param".to_string(),
                    ))
        })
        .expect("unknown route path param diagnostic");

    assert!(diagnostic.message.contains("Available params: id"));
}

#[test]
fn collect_reports_workspace_i18n_missing_key() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("src/components/LoginButton.vue");
    let locale_path = dir.path().join("src/locales/en.json");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::create_dir_all(locale_path.parent().unwrap()).unwrap();
    fs::write(&locale_path, r#"{ "auth": { "login": "Log in" } }"#).unwrap();

    let source = r#"<script setup lang="ts">
const title = t("auth.missing")
</script>"#;
    fs::write(&source_path, source).unwrap();

    let state = state_with_ecosystem_diagnostics();
    let uri = Url::from_file_path(&source_path).unwrap();
    state
        .documents
        .open(uri.clone(), source.to_string(), 1, "vue".to_string());

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.source.as_deref() == Some("vize/ecosystem")
            && diagnostic.code
                == Some(NumberOrString::String(
                    "ecosystem/vue-i18n-no-missing-key".to_string(),
                ))
    }));
}

#[test]
fn collect_short_circuits_dependent_diagnostics_after_script_parse_error() {
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///BrokenScript.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup lang=\"ts\">\nconst count =</script>\n<template>{{ count }}</template>"
            .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);
    let diagnostic_sources: Vec<_> = diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.source.as_deref())
        .collect();

    assert_eq!(diagnostic_sources, vec![sources::SCRIPT_PARSER]);
    assert_eq!(
        diagnostics[0].code,
        Some(NumberOrString::String("script-parse-error".to_string()))
    );
    assert_eq!(diagnostics[0].range.start.line, 1);
}

#[test]
fn collect_accepts_jsx_script_without_false_parse_diagnostic() {
    let state = state_with_lsp_diagnostics(false, false);
    let uri = Url::parse("file:///JsxScript.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup lang=\"jsx\">\nconst count = 1\nconst vnode = <button>{count}</button>\n</script>"
            .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.source.as_deref() == Some(sources::SCRIPT_PARSER))
    );
}

#[test]
fn collect_accepts_tsx_script_without_false_parse_diagnostic() {
    let state = state_with_lsp_diagnostics(false, false);
    let uri = Url::parse("file:///TsxScript.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup lang=\"tsx\">\nconst count = 1\nconst vnode = <button>{count}</button>\n</script>"
            .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.source.as_deref() == Some(sources::SCRIPT_PARSER))
    );
}

#[test]
fn collect_skips_unsupported_script_language() {
    let state = state_with_lsp_diagnostics(false, false);
    let uri = Url::parse("file:///CoffeeScript.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup lang=\"coffee\">\ncount = ->\n</script>".to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.source.as_deref() == Some(sources::SCRIPT_PARSER))
    );
}

#[test]
fn collect_surfaces_jsx_compiler_error_for_broken_tsx() {
    // `has_diagnostics()` gates the whole pipeline; enable lint so the
    // JSX branch is reached (lint itself does not apply to `.tsx`).
    let state = state_with_lsp_diagnostics(true, false);
    let uri = Url::parse("file:///Broken.tsx").unwrap();
    // Unclosed JSX element — the JSX compiler reports a parse error.
    state.documents.open(
        uri.clone(),
        "const a = <div>;".to_string(),
        1,
        "typescriptreact".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    let jsx_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.source.as_deref() == Some(sources::JSX_COMPILER)
                && d.severity == Some(DiagnosticSeverity::ERROR)
        })
        .collect();

    assert!(
        !jsx_errors.is_empty(),
        "expected a JSX compiler error diagnostic, got: {diagnostics:?}"
    );
    // The range must be non-empty and land on the first (only) line.
    let diag = jsx_errors[0];
    assert_eq!(diag.range.start.line, 0);
    assert!(
        diag.range.end >= diag.range.start,
        "diagnostic range must be well-ordered: {:?}",
        diag.range
    );
}

#[test]
fn collect_produces_no_error_for_valid_tsx() {
    let state = state_with_lsp_diagnostics(true, false);
    let uri = Url::parse("file:///Valid.tsx").unwrap();
    state.documents.open(
        uri.clone(),
        "const App = () => <div class=\"a\">{count}</div>;".to_string(),
        1,
        "typescriptreact".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "valid TSX must not produce error diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn collect_produces_no_error_for_valid_jsx() {
    let state = state_with_lsp_diagnostics(true, false);
    let uri = Url::parse("file:///Valid.jsx").unwrap();
    state.documents.open(
        uri.clone(),
        "const App = () => <div class=\"a\">hello</div>;".to_string(),
        1,
        "javascriptreact".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "valid JSX must not produce error diagnostics, got: {diagnostics:?}"
    );
}

// VDOM/Vapor mode directives on JSX/TSX (#1498).
//
// `"use vue:vdom"` / `"use vue:vapor"` select a component's output mode. A
// malformed or conflicting mode directive is a JSX-compiler diagnostic; a
// well-formed one selects the mode and must NOT be mis-diagnosed. These are
// structural (no Corsa bridge / no `jsxTypecheck`), surfaced via the JSX
// compiler lane.
// ----------------------------------------------------------------------

/// A well-formed `"use vue:vapor"` component must type-check the same as the
/// default VDOM mode: the resolved mode is reflected (the component compiles in
/// Vapor mode) without surfacing any spurious diagnostic.
#[test]
fn collect_does_not_misdiagnose_valid_vapor_mode_tsx() {
    let state = state_with_lsp_diagnostics(true, false);
    let uri = Url::parse("file:///Vapor.tsx").unwrap();
    state.documents.open(
        uri.clone(),
        "const Fast = () => {\n  \"use vue:vapor\";\n  return <div class=\"a\">hi</div>;\n};\n"
            .to_string(),
        1,
        "typescriptreact".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity == Some(DiagnosticSeverity::ERROR)),
        "a valid \"use vue:vapor\" component must not be mis-diagnosed, got: {diagnostics:?}"
    );
}

/// A malformed mode directive (`"use vue:vdomx"`, a typo) surfaces as a JSX
/// compiler error with the exact guidance message.
#[test]
fn collect_surfaces_malformed_mode_directive_on_tsx() {
    let state = state_with_lsp_diagnostics(true, false);
    let uri = Url::parse("file:///Typo.tsx").unwrap();
    state.documents.open(
        uri.clone(),
        "const C = () => {\n  \"use vue:vdomx\";\n  return <div>hi</div>;\n};\n".to_string(),
        1,
        "typescriptreact".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    let mode_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.source.as_deref() == Some(sources::JSX_COMPILER))
        .collect();
    assert_eq!(
        mode_errors.len(),
        1,
        "expected exactly one JSX mode diagnostic, got: {diagnostics:?}"
    );
    assert_eq!(
        mode_errors[0].message,
        "unknown JSX mode directive \"use vue:vdomx\": expected \"use vue:vdom\" or \"use vue:vapor\""
    );
    assert_eq!(mode_errors[0].severity, Some(DiagnosticSeverity::ERROR));
    // The squiggle lands on the directive line (line index 1).
    assert_eq!(mode_errors[0].range.start.line, 1);
}

/// Two different mode directives in one component conflict; the later one is
/// reported with the exact "select only one output mode" message.
#[test]
fn collect_surfaces_conflicting_mode_directives_on_tsx() {
    let state = state_with_lsp_diagnostics(true, false);
    let uri = Url::parse("file:///Conflict.tsx").unwrap();
    state.documents.open(
        uri.clone(),
        "const C = () => {\n  \"use vue:vdom\";\n  \"use vue:vapor\";\n  return <div>hi</div>;\n};\n"
            .to_string(),
        1,
        "typescriptreact".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    let mode_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.source.as_deref() == Some(sources::JSX_COMPILER))
        .collect();
    assert_eq!(
        mode_errors.len(),
        1,
        "expected exactly one conflict diagnostic, got: {diagnostics:?}"
    );
    assert_eq!(
        mode_errors[0].message,
        "conflicting JSX mode directives: \"use vue:vapor\" follows \"use vue:vdom\" in the same \
         component; a component can select only one output mode"
    );
    assert_eq!(mode_errors[0].severity, Some(DiagnosticSeverity::ERROR));
}

/// React `.tsx` must be left untouched when `typeChecker.jsxTypecheck` is off:
/// the async pass adds no `vize/types` (type-checker) diagnostics for JSX. With
/// the flag off the JSX type branch is skipped entirely, so even without a
/// Corsa bridge this asserts the gating, not bridge availability.
#[cfg(feature = "native")]
#[test]
fn collect_async_skips_jsx_type_diagnostics_when_flag_off() {
    crate::runtime::block_on(async {
        // typecheck on (so the pipeline runs) but jsxTypecheck left default-off.
        let state = state_with_lsp_diagnostics(true, true);
        assert!(!state.jsx_typecheck_enabled());
        let uri = Url::parse("file:///React.tsx").unwrap();
        state.documents.open(
            uri.clone(),
            "const App = () => <div>{count}</div>;".to_string(),
            1,
            "typescriptreact".to_string(),
        );

        let diagnostics = DiagnosticService::collect_async(&state, &uri).await;

        assert!(
            !diagnostics
                .iter()
                .any(|d| d.source.as_deref() == Some(sources::TYPE_CHECKER)),
            "jsxTypecheck off must not surface any type-checker diagnostics for .tsx, got: {diagnostics:?}"
        );
    });
}

fn lint_subset(diagnostics: &[Diagnostic]) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.source.as_deref(),
                Some(sources::LINTER) | Some(sources::MUSEA)
            )
        })
        .cloned()
        .collect()
}

#[test]
fn collect_lint_only_equals_lint_subset_of_collect() {
    // With both lint and typecheck enabled, collect() produces LINTER and
    // TYPE_CHECKER diagnostics; collect_lint_only() must return exactly the
    // LINTER/MUSEA subset (same diagnostics, same order) and nothing else.
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///OutOfOrder.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<style scoped>.a {}</style>\n<script setup>const count = 1</script>".to_string(),
        1,
        "vue".to_string(),
    );

    let full = DiagnosticService::collect(&state, &uri);
    let lint_only = DiagnosticService::collect_lint_only(&state, &uri);

    // Sanity: the full pipeline really did surface a lint diagnostic here.
    assert!(
        full.iter()
            .any(|d| d.source.as_deref() == Some(sources::LINTER)),
        "expected a lint diagnostic from the full pipeline"
    );
    // The lint-only result is byte-for-byte the lint/musea subset.
    assert_eq!(lint_only, lint_subset(&full));
    // And it carries no non-lint sources (no type-check / parser / sfc-compile).
    assert!(
        lint_only.iter().all(|d| {
            matches!(
                d.source.as_deref(),
                Some(sources::LINTER) | Some(sources::MUSEA)
            )
        }),
        "collect_lint_only must only return lint/musea diagnostics, got: {:?}",
        lint_only
            .iter()
            .map(|d| d.source.as_deref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn collect_lint_only_short_circuits_on_parse_error() {
    // A block parse error gates lint in the full pipeline, so the lint-only
    // path must also return nothing — matching what hover would otherwise see.
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///BrokenTemplateLintOnly.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup lang=\"ts\">const count = 1</script>\n<template><div>{{ count }}</template>"
            .to_string(),
        1,
        "vue".to_string(),
    );

    let full = DiagnosticService::collect(&state, &uri);
    let lint_only = DiagnosticService::collect_lint_only(&state, &uri);

    assert_eq!(lint_only, lint_subset(&full));
    assert!(lint_only.is_empty());
}

#[test]
fn collect_keeps_dependent_diagnostics_after_recoverable_template_repair() {
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///RecoverableTemplateRepair.vue").unwrap();
    state.documents.open(
        uri.clone(),
        r#"<template>
  <span />
  {{ missing }}
</template><style scoped>.a {}</style>
<script setup lang="ts">
const count = 1
</script>
"#
        .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert_diagnostics_snapshot(
        "diagnostics_recoverable_template_repair_keeps_dependent",
        &diagnostics,
    );
}

#[test]
fn collect_lint_only_keeps_lint_after_recoverable_template_repair() {
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///RecoverableTemplateRepairLintOnly.vue").unwrap();
    state.documents.open(
        uri.clone(),
        r#"<template>
  <span />
</template><style scoped>.a {}</style>
<script setup lang="ts">
const count = 1;
</script>
"#
        .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect_lint_only(&state, &uri);

    assert_diagnostics_snapshot(
        "diagnostics_lint_only_recoverable_template_repair_keeps_lint",
        &diagnostics,
    );
}

#[test]
fn collect_lint_only_is_empty_when_lint_disabled() {
    // Hover gates on is_lsp_lint_enabled, but the collector is defensively
    // empty when lint is off even if typecheck is on.
    let state = state_with_lsp_diagnostics(false, true);
    let uri = Url::parse("file:///NoLint.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup>const props = defineProps(['count'])</script><template>{{ props.count }}</template>".to_string(),
        1,
        "vue".to_string(),
    );

    assert!(DiagnosticService::collect_lint_only(&state, &uri).is_empty());
}

// ----------------------------------------------------------------------
// JSX-in-SFC `<script lang="tsx">` (#1498).
//
// A `.vue` whose `<script lang="tsx">` carries a Vue JSX render function must
// be handled as TSX: the embedded JSX is valid, not a script-parse error. The
// script-parse lane resolves the block dialect from `lang`, so the JSX is
// accepted instead of collapsing the SFC to the typed fallback stub. (The
// canon side pins the type-checking lowering; this pins the LSP diagnostics.)
// ----------------------------------------------------------------------

#[test]
fn collect_accepts_jsx_render_fn_in_tsx_script_block() {
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///TsxRenderFn.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup lang=\"tsx\">\nconst label: string = 'hi'\nconst render = () => <button>{label}</button>\n</script>\n"
            .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    // The JSX is valid TSX, so neither the script parser nor the SFC compiler
    // may flag it (a plain-TS parse would have rejected the `<button>`).
    assert!(
        !diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.source.as_deref(),
            Some(sources::SCRIPT_PARSER) | Some(sources::SFC_COMPILER)
        )),
        "JSX in a <script lang=\"tsx\"> block must not raise a parse/compile diagnostic, got: {diagnostics:?}"
    );
}

#[test]
fn collect_still_rejects_jsx_in_plain_ts_script_block() {
    // The dialect is keyed off `lang`: a plain `<script lang="ts">` (no JSX
    // opt-in) with a stray `<button>` is still a real script parse error, so we
    // did not blanket-enable JSX for every SFC script.
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///TsRenderFn.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup lang=\"ts\">\nconst render = () => <button>x</button>\n</script>\n"
            .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.source.as_deref() == Some(sources::SCRIPT_PARSER)),
        "JSX in a plain <script lang=\"ts\"> block must still be a parse error, got: {diagnostics:?}"
    );
}

#[test]
fn collect_short_circuits_dependent_diagnostics_after_template_parse_error() {
    let state = state_with_lsp_diagnostics(true, true);
    let uri = Url::parse("file:///BrokenTemplate.vue").unwrap();
    state.documents.open(
        uri.clone(),
        "<script setup lang=\"ts\">const count = 1</script>\n<template><div>{{ count }}</template>"
            .to_string(),
        1,
        "vue".to_string(),
    );

    let diagnostics = DiagnosticService::collect(&state, &uri);
    let diagnostic_sources: Vec<_> = diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.source.as_deref())
        .collect();

    assert_eq!(diagnostic_sources, vec![sources::TEMPLATE_PARSER]);
}
