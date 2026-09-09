use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Url};

use super::DiagnosticService;
use crate::server::ServerState;

#[test]
fn collect_lints_configured_project_local_rule_options() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("vize.config.json"),
        r##"{
            "lsp": { "lint": true },
            "linter": {
                "preset": "incremental",
                "rules": {
                    "musea/prefer-design-tokens": "error",
                    "script/no-restricted-members": "error"
                },
                "ruleOptions": {
                    "script/no-restricted-members": {
                        "members": [
                            {
                                "object": "window",
                                "property": "localStorage",
                                "message": "Use appStorage."
                            }
                        ]
                    },
                    "musea/prefer-design-tokens": {
                        "tokens": [
                            {
                                "path": "color.primary",
                                "value": "#3b82f6"
                            }
                        ]
                    }
                }
            }
        }"##,
    )
    .unwrap();

    let state = ServerState::new();
    state.load_lsp_config(dir.path());

    let app_uri = Url::from_file_path(dir.path().join("src/App.vue")).unwrap();
    let app_source = r#"<script setup lang="ts">
window.localStorage.getItem("token")
</script>
<template><main></main></template>
"#;
    state.documents.open(
        app_uri.clone(),
        app_source.to_string(),
        1,
        "vue".to_string(),
    );

    let art_uri = Url::from_file_path(dir.path().join("src/Button.art.vue")).unwrap();
    let art_source = r##"<art title="Button" component="./Button.vue">
  <variant name="default"><button class="button">Save</button></variant>
</art>

<style scoped>
.button {
  background: #3b82f6;
}
</style>
"##;
    state.documents.open(
        art_uri.clone(),
        art_source.to_string(),
        1,
        "vue".to_string(),
    );

    let inline_art_uri = Url::from_file_path(dir.path().join("src/InlineArt.vue")).unwrap();
    let inline_art_source = r##"<template><main></main></template>
<art title="Inline" component="./Inline.vue">
<style scoped>
.button {
  color: #3b82f6;
}
</style>
</art>
"##;
    state.documents.open(
        inline_art_uri.clone(),
        inline_art_source.to_string(),
        1,
        "vue".to_string(),
    );

    let app_diagnostics = DiagnosticService::collect_lint_only(&state, &app_uri);
    let art_diagnostics = DiagnosticService::collect_lint_only(&state, &art_uri);
    let inline_art_diagnostics = DiagnosticService::collect_lint_only(&state, &inline_art_uri);

    assert_eq!(
        diagnostic_codes(&app_diagnostics),
        ["script/no-restricted-members"],
        "configured restricted members must run through the LSP lint path"
    );
    assert_eq!(
        diagnostic_codes(&art_diagnostics),
        ["musea/prefer-design-tokens"],
        "configured Musea design tokens must run through the Art LSP lint path"
    );
    assert_eq!(
        diagnostic_codes(&inline_art_diagnostics),
        ["musea/prefer-design-tokens"],
        "configured Musea design tokens must run through inline <art> blocks"
    );
    assert_eq!(app_diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(art_diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(
        inline_art_diagnostics[0].severity,
        Some(DiagnosticSeverity::ERROR)
    );
}

#[test]
fn collect_lints_configured_html_self_closing_options() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("vize.config.json"),
        r##"{
            "languageServer": { "lint": true },
            "linter": {
                "preset": "incremental",
                "rules": {
                    "vue/html-self-closing": "error"
                },
                "ruleOptions": {
                    "vue/html-self-closing": {
                        "html": {
                            "void": "any",
                            "normal": "never",
                            "component": "any"
                        },
                        "svg": "any",
                        "math": "any"
                    }
                }
            }
        }"##,
    )
    .unwrap();

    let state = ServerState::new();
    state.load_lsp_config(dir.path());

    let uri = Url::from_file_path(dir.path().join("src/SelfClosing.vue")).unwrap();
    let source = r#"<template>
  <img>
  <div />
  <MyWidget></MyWidget>
</template>
"#;
    state
        .documents
        .open(uri.clone(), source.to_string(), 1, "vue".to_string());

    let diagnostics = DiagnosticService::collect_lint_only(&state, &uri);
    assert_eq!(
        diagnostic_codes(&diagnostics),
        ["vue/html-self-closing"],
        "configured html-self-closing options must run through the LSP lint path"
    );
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
}

#[test]
fn collect_lints_configured_misskey_vue_rule_options() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("vize.config.json"),
        r##"{
            "languageServer": { "lint": true },
            "linter": {
                "preset": "incremental",
                "rules": {
                    "vue/no-mutating-props": "error",
                    "vue/sfc-element-order": "error",
                    "vue/v-on-event-hyphenation": "error",
                    "vue/attribute-hyphenation": "error"
                },
                "ruleOptions": {
                    "vue/no-mutating-props": { "shallowOnly": true },
                    "vue/sfc-element-order": {
                        "order": ["template", "script:not([setup])", "script[setup]", "style"]
                    },
                    "vue/v-on-event-hyphenation": "never",
                    "vue/attribute-hyphenation": "never"
                }
            }
        }"##,
    )
    .unwrap();

    let state = ServerState::new();
    state.load_lsp_config(dir.path());

    let uri = Url::from_file_path(dir.path().join("src/RuleOptions.vue")).unwrap();
    let source = r#"<template>
  <MyWidget my-prop="value" @my-event="handler" />
</template>

<script setup lang="ts">
const props = defineProps<{ count: number; profile: { name: string } }>();
props.profile.name = 'Ada';
props.count = 1;
</script>

<script lang="ts">
export default {};
</script>

<style></style>
"#;
    state
        .documents
        .open(uri.clone(), source.to_string(), 1, "vue".to_string());

    let diagnostics = DiagnosticService::collect_lint_only(&state, &uri);
    let mut codes = diagnostic_codes(&diagnostics);
    codes.sort_unstable();
    assert_eq!(
        codes,
        [
            "vue/attribute-hyphenation",
            "vue/no-mutating-props",
            "vue/sfc-element-order",
            "vue/v-on-event-hyphenation",
        ],
        "configured Vue options must run through the LSP lint path"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| { diagnostic.severity == Some(DiagnosticSeverity::ERROR) })
    );
}

#[test]
fn collect_musea_lint_respects_disabled_linter_config() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("vize.config.json"),
        r##"{
            "languageServer": { "lint": true },
            "linter": {
                "enabled": false,
                "ruleOptions": {
                    "musea/prefer-design-tokens": {
                        "tokens": [
                            {
                                "path": "color.primary",
                                "value": "#3b82f6"
                            }
                        ]
                    }
                }
            }
        }"##,
    )
    .unwrap();

    let state = ServerState::new();
    state.load_lsp_config(dir.path());

    let art_uri = Url::from_file_path(dir.path().join("src/Button.art.vue")).unwrap();
    let art_source = r##"<art title="Button" component="./Button.vue">
  <variant name="default"><button class="button">Save</button></variant>
</art>

<style scoped>
.button {
  background: #3b82f6;
}
</style>
"##;
    state.documents.open(
        art_uri.clone(),
        art_source.to_string(),
        1,
        "vue".to_string(),
    );

    assert_eq!(
        diagnostic_codes(&DiagnosticService::collect_lint_only(&state, &art_uri)),
        Vec::<String>::new()
    );
}

fn diagnostic_codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| match diagnostic.code.as_ref() {
            Some(NumberOrString::String(code)) => Some(code.clone()),
            _ => None,
        })
        .collect()
}
