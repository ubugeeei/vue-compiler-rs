use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use vize_s0::cstr;

fn temp_project_dir() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(
        cstr!(
            "vize-lint-rule-options-casing-{}-{nonce}",
            std::process::id()
        )
        .as_str(),
    )
}

fn write_project_file(root: &Path, path: &str, content: &str) {
    let file_path = root.join(path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(file_path, content).unwrap();
}

fn output_details(output: &Output) -> vize_s0::String {
    let stdout = std::str::from_utf8(&output.stdout).unwrap_or("<non-utf8 stdout>");
    let stderr = std::str::from_utf8(&output.stderr).unwrap_or("<non-utf8 stderr>");
    cstr!("stdout:\n{}\nstderr:\n{}", stdout, stderr)
}

#[test]
fn lint_rule_options_configure_template_and_event_casing() {
    let project_root = temp_project_dir();
    write_project_file(
        &project_root,
        "src/Casing.vue",
        r#"<template>
  <my-widget />
  <MyWidget />
</template>

<script setup lang="ts">
import MyWidget from './MyWidget.vue';

const emit = defineEmits<{ 'keep-original': []; keepOriginal: [] }>();

emit('keep-original');
emit('keepOriginal');
</script>
"#,
    );
    write_project_file(
        &project_root,
        "vize.config.json",
        r#"{
  "linter": {
    "preset": "incremental",
    "rules": {
      "vue/component-name-in-template-casing": "error",
      "script/custom-event-name-casing": "error"
    },
    "ruleOptions": {
      "vue/component-name-in-template-casing": { "casing": "kebab-case" },
      "script/custom-event-name-casing": { "casing": "kebab-case" }
    }
  }
}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vize"))
        .current_dir(&project_root)
        .args([
            "lint",
            "--config",
            "vize.config.json",
            "--format",
            "json",
            "src/Casing.vue",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", output_details(&output));
    assert_eq!(output.stderr, b"", "{}", output_details(&output));

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let report: serde_json::Value = serde_json::from_str(stdout).unwrap();
    assert_eq!(
        report,
        serde_json::json!([
            {
                "file": "src/Casing.vue",
                "messages": [
                    {
                        "ruleId": "vue/component-name-in-template-casing",
                        "ruleDocsPath": "docs/content/rules/vue.md",
                        "severity": 2,
                        "message": "[vize:vue/component-name-in-template-casing] Component should use kebab-case",
                        "line": 3,
                        "column": 3,
                        "endLine": 3,
                        "endColumn": 15,
                        "help": "Use kebab-case for component names"
                    },
                    {
                        "ruleId": "script/custom-event-name-casing",
                        "ruleDocsPath": "docs/content/rules/type-and-script.md",
                        "severity": 2,
                        "message": "[vize:script/custom-event-name-casing] Custom event name 'keepOriginal' is not kebab-case.",
                        "line": 12,
                        "column": 6,
                        "endLine": 12,
                        "endColumn": 20,
                        "help": "Rename this emitted event to kebab-case (e.g. my-event)."
                    }
                ],
                "errorCount": 2,
                "warningCount": 0
            }
        ])
    );

    let _ = fs::remove_dir_all(project_root);
}

#[test]
fn lint_rule_options_configure_html_self_closing() {
    let project_root = temp_project_dir();
    write_project_file(
        &project_root,
        "src/SelfClosing.vue",
        r#"<template>
  <img>
  <div />
  <MyWidget></MyWidget>
  <svg><path></path></svg>
</template>
"#,
    );
    write_project_file(
        &project_root,
        "vize.config.json",
        r#"{
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
}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vize"))
        .current_dir(&project_root)
        .args([
            "lint",
            "--config",
            "vize.config.json",
            "--format",
            "json",
            "src/SelfClosing.vue",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", output_details(&output));
    assert_eq!(output.stderr, b"", "{}", output_details(&output));

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let report: serde_json::Value = serde_json::from_str(stdout).unwrap();
    let messages = report[0]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1, "{stdout}");
    assert_eq!(messages[0]["ruleId"], "vue/html-self-closing");
    assert_eq!(
        messages[0]["message"],
        "[vize:vue/html-self-closing] Element must not use self-closing syntax"
    );

    let _ = fs::remove_dir_all(project_root);
}

#[test]
fn lint_rule_options_configure_misskey_vue_rules() {
    let project_root = temp_project_dir();
    write_project_file(
        &project_root,
        "src/RuleOptions.vue",
        r#"<template>
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
"#,
    );
    write_project_file(
        &project_root,
        "vize.config.json",
        r#"{
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
}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vize"))
        .current_dir(&project_root)
        .args([
            "lint",
            "--config",
            "vize.config.json",
            "--format",
            "json",
            "src/RuleOptions.vue",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", output_details(&output));
    assert_eq!(output.stderr, b"", "{}", output_details(&output));

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let report: serde_json::Value = serde_json::from_str(stdout).unwrap();
    let messages = report[0]["messages"].as_array().unwrap();
    let mut rule_ids = messages
        .iter()
        .map(|message| message["ruleId"].as_str().unwrap())
        .collect::<Vec<_>>();
    rule_ids.sort_unstable();
    assert_eq!(
        rule_ids,
        [
            "vue/attribute-hyphenation",
            "vue/no-mutating-props",
            "vue/sfc-element-order",
            "vue/v-on-event-hyphenation",
        ]
    );
    let no_mutating_props_messages = messages
        .iter()
        .filter(|message| message["ruleId"].as_str() == Some("vue/no-mutating-props"))
        .map(|message| message["message"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        no_mutating_props_messages,
        [
            "[vize:vue/no-mutating-props] Unexpected mutation of prop 'props.count' in <script setup>"
        ],
        "{stdout}"
    );

    let _ = fs::remove_dir_all(project_root);
}
