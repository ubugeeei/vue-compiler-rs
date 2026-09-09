use super::tsconfig_paths::{parse_jsonc_value, strip_json_comments};
use super::{AUTO_IMPORT_STUBS_FILE, SHARED_HELPERS_FILE, VUE_MODULE_STUBS_FILE, VirtualProject};
use crate::{batch::Diagnostic, batch::SfcBlockType, virtual_ts::VirtualTsOptions};
use std::{fs, path::Path, path::PathBuf};
use vize_atelier_core::TemplateSyntaxMode;
use vize_carton::cstr;
mod alias_rewrite;
mod base_url;
mod declaration_root_dir;
mod graphql_generated;
mod macro_scope;
mod materialization_namespace;
mod module_augmentations;
mod project_isolation;
mod ref_arity;
mod setup_props;
mod source_types;
mod tsconfig_extends;
mod tsconfig_native_options;
mod windows_paths;
mod workspace_package_dependency_shadows;
mod workspace_package_routes;
fn unique_case_dir(name: &str) -> PathBuf {
    static NEXT_CASE_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let case_id = NEXT_CASE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("vize-tests")
        .join("tests")
        .join(cstr!("{name}-{}-{case_id}", std::process::id()).as_str())
}
fn assert_ts_parses(source: &str) {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::ts()).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "virtual TS should parse without errors: {:?}",
        parsed.diagnostics
    );
}
fn assert_tsx_parses(source: &str) {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::tsx()).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "virtual TSX should parse without errors: {:?}",
        parsed.diagnostics
    );
}
#[derive(Debug)]
#[allow(dead_code)]
struct DiagnosticSnapshot<'a> {
    line: u32,
    column: u32,
    message: &'a str,
    code: Option<u32>,
    severity: u8,
    block_type: Option<SfcBlockType>,
}
fn diagnostic_snapshot(diagnostics: &[Diagnostic]) -> Vec<DiagnosticSnapshot<'_>> {
    diagnostics
        .iter()
        .map(|diagnostic| DiagnosticSnapshot {
            line: diagnostic.line,
            column: diagnostic.column,
            message: diagnostic.message.as_str(),
            code: diagnostic.code,
            severity: diagnostic.severity,
            block_type: diagnostic.block_type,
        })
        .collect()
}
fn snapshot_text(source: &str) -> std::string::String {
    let mut output = std::string::String::with_capacity(source.len());
    for (index, line) in source.split('\n').enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(line.trim_end_matches(|ch| ch == ' ' || ch == '\t'));
    }
    output
}
#[test]
fn test_materialize_writes_inert_vue_module_stub_file() {
    let case_dir = unique_case_dir("vue-module-stubs");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let main_path = src_dir.join("main.ts");
    fs::write(&main_path, "import App from './App.vue';\nvoid App;\n").unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_path(&main_path).unwrap();
    project.materialize().unwrap();

    let stubs = fs::read_to_string(project.virtual_root().join("__vize_vue_modules.d.ts")).unwrap();
    insta::assert_snapshot!("vue_module_stubs_inert", stubs.as_str());

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn tsx_script_block_is_type_checked_not_collapsed_to_fallback_stub() {
    // #1498: a `.vue` whose `<script setup lang="tsx">` contains JSX must be
    // lowered to real virtual TypeScript so the script body reaches the type
    // checker. Before the `lang`-aware script parse, the JSX (`<button>…`) was
    // parsed as plain TS, raised a spurious parse error, and collapsed the whole
    // SFC to the `__vize_component: any` fallback stub — silently dropping all
    // type-checking of the script. Pin that the JSX dialect is now honored: the
    // generated virtual TS is the real module (byte-identical across the batch
    // and single-document generators), not the stub.
    let case_dir = unique_case_dir("tsx-script-not-stub");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("App.vue");
    let vue_content = "<script setup lang=\"tsx\">\nconst label: string = 'hi'\nconst vnode = <button>{label}</button>\n</script>\n";
    fs::write(&vue_path, vue_content).unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();
    let virtual_file = project.find_by_original(&vue_path).unwrap();
    assert_eq!(
        virtual_file.virtual_path,
        project.virtual_root().join("src/App.vue.tsx")
    );
    let batch_content = virtual_file.content.clone();

    // The whole-SFC fallback stub emitted when a hard parse error aborts codegen.
    let fallback_stub = "declare const __vize_component: any;\nexport default __vize_component;\n";
    assert_ne!(
        batch_content.as_str(),
        fallback_stub,
        "tsx script with JSX collapsed to the fallback stub instead of being type-checked"
    );

    // The single-document (LSP/socket) generator must agree byte-for-byte with
    // the batch generator, so the editor type-checks the JSX script identically.
    let rewriter = super::super::import_rewriter::ImportRewriter::new();
    let shared = super::generate_vue_document_virtual_ts(
        &vue_path,
        vue_content,
        &VirtualTsOptions::default(),
        &rewriter,
        true,
    )
    .unwrap();
    assert_eq!(shared.code.as_str(), batch_content.as_str());
    assert_eq!(shared.virtual_suffix, ".tsx");
    assert_tsx_parses(batch_content.as_str());

    project.materialize().unwrap();
    assert!(project.virtual_root().join("src/App.vue.tsx").exists());
    let tsconfig: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(project.virtual_root().join("tsconfig.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        tsconfig["compilerOptions"]["jsx"],
        serde_json::json!("preserve")
    );
    assert_eq!(
        tsconfig["compilerOptions"]["jsxImportSource"],
        serde_json::json!("vue")
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn shared_document_generator_is_byte_identical_to_batch_pipeline() {
    // Issue #1389: the Corsa socket single-document path and the `vize check`
    // batch path must produce identical virtual TS for the same input. With the
    // shared preamble hoisted (as the batch path materializes it), the shared
    // single-document generator must match `register_vue_file` byte-for-byte.
    let case_dir = unique_case_dir("shared-doc-vs-batch");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("App.vue");
    let vue_content = r#"<script setup lang="ts">
import Child from './Child.vue'
const count = 1
const label = 'hi'
</script>

<template>
  <Child :count="count" />
  <span>{{ label }}</span>
</template>
"#;
    fs::write(&vue_path, vue_content).unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();
    let batch_content = project.find_by_original(&vue_path).unwrap().content.clone();

    let rewriter = super::super::import_rewriter::ImportRewriter::new();
    let shared = super::generate_vue_document_virtual_ts(
        &vue_path,
        vue_content,
        &VirtualTsOptions::default(),
        &rewriter,
        true,
    )
    .unwrap();

    assert_eq!(shared.code.as_str(), batch_content.as_str());

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_register_vue_file_rewrites_child_imports() {
    let case_dir = unique_case_dir("register-vue");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("App.vue");
    let vue_content = r#"<script setup lang="ts">
import Child from './Child.vue'
const count = 1
</script>

<template>
  <Child :count="count" />
</template>
"#;
    fs::write(&vue_path, vue_content).unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();

    let virtual_file = project.find_by_original(&vue_path).unwrap();
    insta::assert_snapshot!(virtual_file.content.as_str());

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_register_vue_file_rewrites_options_api_export_default() {
    let case_dir = unique_case_dir("options-api-export-default");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("OptionsApi.vue");
    let vue_content = r#"<script lang="ts">
import { defineComponent } from "vue";

export default defineComponent({
  name: "OptionsApi",
});
</script>

<template>
  <div>hello</div>
</template>
"#;
    fs::write(&vue_path, vue_content).unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();

    let virtual_file = project.find_by_original(&vue_path).unwrap();
    insta::assert_snapshot!(virtual_file.content.as_str());
    assert_ts_parses(virtual_file.content.as_str());

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_register_vue_file_respects_template_syntax_quirks() {
    let case_dir = unique_case_dir("template-syntax-quirks");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("App.vue");
    let vue_content = r#"<script setup lang="ts">
defineProps<{
  test: string
}>()
</script>

<template>
  <div />
</template>
"#;
    fs::write(&vue_path, vue_content).unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.set_template_syntax(TemplateSyntaxMode::Quirks);
    project.register_path(&vue_path).unwrap();

    assert!(
        project.diagnostics().is_empty(),
        "{:#?}",
        project.diagnostics()
    );
    assert!(project.find_by_original(&vue_path).is_some());

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_register_vue_file_reports_script_parse_error_with_fallback() {
    let case_dir = unique_case_dir("script-parse-error");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("Broken.vue");
    let vue_content = r#"<script setup lang="ts">
const count =
</script>

<template>
  <div>{{ count }}</div>
</template>
"#;

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();

    let diagnostics = project.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    insta::assert_debug_snapshot!(
        "script_parse_error_diagnostics",
        diagnostic_snapshot(diagnostics)
    );

    let virtual_file = project.find_by_original(&vue_path).unwrap();
    insta::assert_snapshot!(
        "script_parse_error_fallback_virtual_ts",
        snapshot_text(virtual_file.content.as_str())
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_register_vue_file_reports_props_destructure_default_type_mismatch() {
    // Regression for: `const { msg = 0 } = defineProps<{ msg?: string }>()` should
    // surface in `vize check`. TypeScript itself does not flag the mismatch
    // (destructure defaults widen the binding's type), so the diagnostic has
    // to come from the SFC compiler's validator.
    let case_dir = unique_case_dir("props-destructure-default-type");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("Bad.vue");
    let vue_content = r#"<script setup lang="ts">
const { msg = 0 } = defineProps<{ msg?: string }>();
</script>

<template>
  <div>{{ msg }}</div>
</template>
"#;

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();

    let diagnostics = project.diagnostics();
    assert_eq!(diagnostics.len(), 1, "expected one SFC compile diagnostic");
    insta::assert_debug_snapshot!(
        "props_destructure_default_type_mismatch_diagnostics",
        diagnostic_snapshot(diagnostics)
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_register_vue_file_allows_matching_props_destructure_default() {
    let case_dir = unique_case_dir("props-destructure-default-ok");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("Good.vue");
    let vue_content = r#"<script setup lang="ts">
const { msg = "ok" } = defineProps<{ msg?: string }>();
</script>

<template>
  <div>{{ msg }}</div>
</template>
"#;

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();

    assert!(
        project.diagnostics().is_empty(),
        "no diagnostics expected for matching default, got: {:?}",
        project.diagnostics()
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_register_vue_file_reports_template_parse_error_with_fallback() {
    let case_dir = unique_case_dir("template-parse-error");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("BrokenTemplate.vue");
    let vue_content = r#"<script setup lang="ts">
const count = 1
</script>

<template><div>{{ count }}</template>
"#;

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();

    let diagnostics = project.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    insta::assert_debug_snapshot!(
        "template_parse_error_diagnostics",
        diagnostic_snapshot(diagnostics)
    );

    let virtual_file = project.find_by_original(&vue_path).unwrap();
    insta::assert_snapshot!(
        "template_parse_error_fallback_virtual_ts",
        snapshot_text(virtual_file.content.as_str())
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_register_vue_file_recovery_diagnostic_does_not_collapse_to_fallback() {
    // Regression (#1065/#1090): the template here triggers only recovery-level
    // parser diagnostics — a self-closing non-void HTML element (`<div />`,
    // rewritten as an empty element) and a self-closing SVG `<path/>` inside
    // `<svg>` (which must not be flagged at all). Neither is a hard error, so
    // the virtual TS must remain real codegen, NOT the
    // `declare const __vize_component: any` stub.
    let case_dir = unique_case_dir("template-recovery-no-fallback");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("Recoverable.vue");
    let vue_content = r#"<script setup lang="ts">
const count = 1
</script>

<template>
  <div />
  <svg viewBox="0 0 24 24"><path d="M0 0h24v24H0z" /></svg>
  <span>{{ count }}</span>
</template>
"#;

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_vue_file(&vue_path, vue_content).unwrap();

    assert!(
        project.diagnostics().is_empty(),
        "recovery-level template diagnostics must not surface, got: {:?}",
        project.diagnostics()
    );

    let virtual_file = project.find_by_original(&vue_path).unwrap();
    insta::assert_snapshot!(
        "template_recovery_no_fallback_virtual_ts",
        snapshot_text(virtual_file.content.as_str())
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_virtual_ts_exposes_props_from_reexported_vue_interface() {
    let case_dir = unique_case_dir("reexported-vue-interface-props");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let base = src_dir.join("Base.vue");
    let index = src_dir.join("index.ts");
    let child = src_dir.join("Child.vue");
    let parent = src_dir.join("ParentWidget.vue");

    fs::write(
        &base,
        r#"<script lang="ts">
export interface BaseProps {
  as?: string;
  asChild?: boolean;
}
</script>
<template><div></div></template>"#,
    )
    .unwrap();
    fs::write(&index, format!(r#"export * from "{}";"#, base.display())).unwrap();
    fs::write(
        &child,
        r#"<script setup lang="ts">
defineProps<{ as?: string; asChild?: boolean }>();
</script>
<template><div></div></template>"#,
    )
    .unwrap();
    fs::write(
        &parent,
        r#"<script lang="ts">
import type { BaseProps } from "./index";

export interface ParentWidgetProps extends BaseProps {}
</script>
<script setup lang="ts">
import Child from "./Child.vue";

const props = defineProps<ParentWidgetProps>();
</script>
<template>
  <Child :as="as" :as-child="props.asChild" />
</template>"#,
    )
    .unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project
        .register_paths(&[base, index, child, parent.clone()])
        .unwrap();

    let virtual_parent = project.find_by_original(&parent).unwrap();
    assert_ts_parses(&virtual_parent.content);
    insta::assert_snapshot!(
        "reexported_vue_interface_props_virtual_ts",
        snapshot_text(virtual_parent.content.as_str())
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_virtual_ts_preserves_ts_as_assertions_when_prop_is_named_as() {
    let case_dir = unique_case_dir("template-as-assertion-prop");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let vue_path = src_dir.join("App.vue");
    fs::write(
        &vue_path,
        r#"<script setup lang="ts">
defineProps<{
  as?: string
}>()

const value = 'demo'
const onFocus = (target: HTMLElement) => {
  target.dataset.focused = 'true'
}
</script>

<template>
  <div
    :data-value="(value as any)"
    :style="{
      ['--demo-value' as any]: value,
    }"
    v-on="{
      focusin: (event: FocusEvent) => {
        onFocus(event.target as HTMLElement)
      },
    }"
  ></div>
</template>
"#,
    )
    .unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project
        .register_vue_file(&vue_path, &fs::read_to_string(&vue_path).unwrap())
        .unwrap();

    let virtual_file = project.find_by_original(&vue_path).unwrap();
    assert_ts_parses(&virtual_file.content);
    insta::assert_snapshot!(
        "template_as_assertion_prop_virtual_ts",
        snapshot_text(virtual_file.content.as_str())
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_materialize_writes_tsconfig_and_virtual_files() {
    let case_dir = unique_case_dir("materialize");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("App.vue");
    fs::write(
        &vue_path,
        r#"<script setup lang="ts">
const message = 'Hello'
</script>

<template>
  <div>{{ message }}</div>
</template>
"#,
    )
    .unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    let mut options = VirtualTsOptions::default();
    options
        .auto_import_stubs
        .push("declare function autoGenerated(): string;".into());
    project.set_virtual_ts_options(options);
    project.register_path(&vue_path).unwrap();
    project.materialize().unwrap();

    let virtual_vue_path = project.virtual_root().join("src/App.vue.ts");
    let tsconfig_path = project.virtual_root().join("tsconfig.json");
    let auto_imports_path = project.virtual_root().join("__vize_auto_imports.d.ts");

    assert!(virtual_vue_path.exists());
    assert!(tsconfig_path.exists());
    assert!(auto_imports_path.exists());
    insta::assert_snapshot!(
        "materialize_virtual_vue",
        snapshot_text(fs::read_to_string(&virtual_vue_path).unwrap().as_str())
    );
    insta::assert_snapshot!(
        "materialize_auto_imports",
        fs::read_to_string(&auto_imports_path).unwrap().as_str()
    );
    insta::assert_snapshot!(
        "materialize_tsconfig",
        fs::read_to_string(&tsconfig_path).unwrap().as_str()
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_materialize_writes_relative_json_modules() {
    let case_dir = unique_case_dir("materialize-json-modules");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    let token_dir = src_dir.join("tokens/source");
    fs::create_dir_all(&token_dir).unwrap();
    let ts_path = src_dir.join("tokens.ts");
    let json_path = token_dir.join("colors.tokens.json");
    fs::write(
        &ts_path,
        "import colors from './tokens/source/colors.tokens.json'\nvoid colors\n",
    )
    .unwrap();
    fs::write(&json_path, "{\"primary\":\"#0057ff\"}\n").unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_path(&ts_path).unwrap();
    project.materialize().unwrap();

    let virtual_json_path = project
        .virtual_root()
        .join("src/tokens/source/colors.tokens.json");
    assert_eq!(
        fs::read_to_string(&virtual_json_path).unwrap(),
        "{\"primary\":\"#0057ff\"}\n"
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn materialize_skips_rewriting_unchanged_files() {
    let case_dir = unique_case_dir("materialize-skip-unchanged");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("App.vue");
    fs::write(
        &vue_path,
        r#"<script setup lang="ts">
const message = 'Hello'
</script>

<template>
  <div>{{ message }}</div>
</template>
"#,
    )
    .unwrap();
    let ts_path = src_dir.join("tokens.ts");
    fs::write(
        &ts_path,
        "import colors from './colors.tokens.json'\nvoid colors\n",
    )
    .unwrap();
    fs::write(
        src_dir.join("colors.tokens.json"),
        "{\"primary\":\"#0057ff\"}\n",
    )
    .unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_path(&vue_path).unwrap();
    project.register_path(&ts_path).unwrap();
    project.materialize().unwrap();

    // One bulk virtual file and one passthrough mirror; rewriting either on
    // the warm rerun below would bump its mtime back to "now".
    let virtual_root = project.virtual_root().to_path_buf();
    let tracked = [
        virtual_root.join("src/App.vue.ts"),
        virtual_root.join("src/colors.tokens.json"),
    ];
    let stale_mtime =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    for path in &tracked {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(stale_mtime).unwrap();
    }

    project.materialize().unwrap();

    for path in &tracked {
        assert_eq!(
            fs::metadata(path).unwrap().modified().unwrap(),
            stale_mtime,
            "unchanged file should not be rewritten on warm rerun: {}",
            path.display()
        );
    }

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn materialize_prunes_stale_virtual_project_entries() {
    let case_dir = unique_case_dir("materialize-gc");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let vue_path = src_dir.join("App.vue");
    fs::write(
        &vue_path,
        r#"<script setup lang="ts">
const message = 'Hello'
</script>

<template>
  <div>{{ message }}</div>
</template>
"#,
    )
    .unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    let mut options = VirtualTsOptions::default();
    options
        .auto_import_stubs
        .push("declare function autoGenerated(): string;".into());
    project.set_virtual_ts_options(options);
    project.register_path(&vue_path).unwrap();
    project.materialize().unwrap();

    let virtual_root = project.virtual_root().to_path_buf();
    let stale_file = virtual_root.join("src/Old.vue.ts");
    let stale_dir_file = virtual_root.join("stale/nested/Unused.vue.ts");
    let stale_dts_config = virtual_root.join("tsconfig.declaration.json");
    let stale_package = virtual_root.join("node_modules/unused/package.json");
    fs::write(&stale_file, "export default {}").unwrap();
    fs::create_dir_all(stale_dir_file.parent().unwrap()).unwrap();
    fs::write(&stale_dir_file, "export default {}").unwrap();
    fs::write(&stale_dts_config, "{}").unwrap();
    fs::create_dir_all(stale_package.parent().unwrap()).unwrap();
    fs::write(&stale_package, "{}").unwrap();
    #[cfg(unix)]
    {
        let expected_virtual_file = virtual_root.join("src/App.vue.ts");
        let hijack_target = case_dir.join("hijack.ts");
        fs::write(&hijack_target, "hijacked").unwrap();
        fs::remove_file(&expected_virtual_file).unwrap();
        std::os::unix::fs::symlink(&hijack_target, &expected_virtual_file).unwrap();
    }

    let mut next_project = VirtualProject::new(&case_dir).unwrap();
    next_project.register_path(&vue_path).unwrap();
    next_project.materialize().unwrap();

    assert!(!stale_file.exists());
    assert!(!stale_dir_file.exists());
    assert!(!stale_dir_file.parent().unwrap().exists());
    assert!(!stale_dts_config.exists());
    assert!(!virtual_root.join(AUTO_IMPORT_STUBS_FILE).exists());
    assert!(!stale_package.exists());
    assert!(!stale_package.parent().unwrap().exists());
    assert!(virtual_root.join("src/App.vue.ts").exists());
    #[cfg(unix)]
    {
        let virtual_file_metadata =
            fs::symlink_metadata(virtual_root.join("src/App.vue.ts")).unwrap();
        assert!(!virtual_file_metadata.file_type().is_symlink());
        assert_eq!(
            fs::read_to_string(case_dir.join("hijack.ts")).unwrap(),
            "hijacked"
        );
    }
    assert!(virtual_root.join(VUE_MODULE_STUBS_FILE).exists());
    assert!(virtual_root.join("tsconfig.json").exists());

    // The hoisted preamble file survives re-materialization, carries the
    // shared declarations, and is listed in the generated tsconfig include.
    let helpers_path = virtual_root.join(SHARED_HELPERS_FILE);
    assert!(helpers_path.exists());
    let helpers_content = fs::read_to_string(&helpers_path).unwrap();
    insta::assert_snapshot!("pruned_helpers", helpers_content.as_str());
    let tsconfig_content = fs::read_to_string(virtual_root.join("tsconfig.json")).unwrap();
    insta::assert_snapshot!("pruned_tsconfig", tsconfig_content.as_str());
    // The generated module relies on the hoisted preamble instead of
    // embedding it (no per-file `declare global` augmentation).
    let generated = fs::read_to_string(virtual_root.join("src/App.vue.ts")).unwrap();
    insta::assert_snapshot!("pruned_generated_vue", snapshot_text(generated.as_str()));

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn materialized_tsconfig_preserves_original_path_option_bases() {
    let case_dir = unique_case_dir("tsconfig-path-bases");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        case_dir.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"]
    },
    "rootDirs": ["src", "generated"],
    "typeRoots": ["types"]
  }
}"#,
    )
    .unwrap();
    let vue_path = src_dir.join("App.vue");
    fs::write(
        &vue_path,
        "<script setup lang=\"ts\">const count = 1</script>",
    )
    .unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_path(&vue_path).unwrap();
    project.materialize().unwrap();

    let tsconfig_path = project.virtual_root().join("tsconfig.json");
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tsconfig_path).unwrap()).unwrap();
    let compiler_options = value["compilerOptions"].as_object().unwrap();

    assert_eq!(compiler_options["strict"], serde_json::Value::Bool(true));
    assert_eq!(
        compiler_options["allowImportingTsExtensions"],
        serde_json::Value::Bool(true)
    );
    for option in ["baseUrl", "rootDir", "rootDirs"] {
        assert!(
            !compiler_options.contains_key(option),
            "{option} is path-sensitive and must not leak into the mirror"
        );
    }
    // Custom type roots are re-anchored like `paths`: mirror copy first, real
    // source tree as fallback, so `types: [...]` entries keep resolving.
    assert_eq!(
        compiler_options["typeRoots"],
        serde_json::json!(["./types", "../../../../types"])
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn materialized_tsconfig_reanchors_paths_into_virtual_mirror() {
    let case_dir = unique_case_dir("tsconfig-paths-reanchor");
    let _ = fs::remove_dir_all(&case_dir);
    let src_dir = case_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        case_dir.join("tsconfig.json"),
        r##"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"],
      "#shared": ["./shared/index.ts"]
    }
  }
}"##,
    )
    .unwrap();
    let vue_path = src_dir.join("App.vue");
    fs::write(
        &vue_path,
        "<script setup lang=\"ts\">const count = 1</script>",
    )
    .unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_path(&vue_path).unwrap();
    project.materialize().unwrap();

    let tsconfig_path = project.virtual_root().join("tsconfig.json");
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tsconfig_path).unwrap()).unwrap();
    let paths = value["compilerOptions"]["paths"].as_object().unwrap();

    // Mirror candidate first, then the real-tree fallback, then a trailing
    // `.vue.ts` mirror candidate for extensionless SFC aliases (#3300).
    assert_eq!(
        paths["@/*"],
        serde_json::json!(["./src/*", "../../../../src/*", "./src/*.vue.ts"])
    );
    assert_eq!(
        paths["#shared"],
        serde_json::json!(["./shared/index.ts", "../../../../shared/index.ts"])
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn materialized_tsconfig_reanchors_extended_paths_from_declaring_config_dir() {
    let case_dir = unique_case_dir("tsconfig-extended-paths-reanchor");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(case_dir.join(".nuxt")).unwrap();
    fs::create_dir_all(case_dir.join("app/components")).unwrap();
    fs::write(
        case_dir.join(".nuxt/tsconfig.json"),
        r##"{
  "compilerOptions": {
    "paths": {
      "~/*": ["../app/*"],
      "#imports": ["./imports"]
    }
  }
}"##,
    )
    .unwrap();
    fs::write(
        case_dir.join("tsconfig.json"),
        r#"{
  "extends": "./.nuxt/tsconfig.json"
}"#,
    )
    .unwrap();
    let vue_path = case_dir.join("app/components/App.vue");
    fs::write(
        &vue_path,
        "<script setup lang=\"ts\">const count = 1</script>",
    )
    .unwrap();

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.register_path(&vue_path).unwrap();
    project.materialize().unwrap();

    let tsconfig_path = project.virtual_root().join("tsconfig.json");
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tsconfig_path).unwrap()).unwrap();
    let paths = value["compilerOptions"]["paths"].as_object().unwrap();

    let app = ["./app/*", "../../../../app/*", "./app/*.vue.ts"];
    let imports = [
        "./.nuxt/imports",
        "../../../../.nuxt/imports",
        "./.nuxt/imports.vue.ts",
    ];
    assert_eq!(paths["~/*"], serde_json::json!(app));
    assert_eq!(paths["#imports"], serde_json::json!(imports));

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn test_parse_jsonc_value_handles_comments_and_trailing_commas() {
    let value = parse_jsonc_value(
        r#"{
  // comment
  "compilerOptions": {
    "strict": true,
    "paths": {
      "@/*": ["src/*",],
    },
  },
}"#,
    )
    .unwrap();

    assert_eq!(
        value["compilerOptions"]["paths"]["@/*"][0],
        serde_json::Value::String("src/*".into())
    );
}

#[test]
fn test_strip_json_comments_preserves_strings() {
    let stripped = strip_json_comments(r#"{ "url": "https://example.com" }"#);
    insta::assert_snapshot!(stripped.as_str());
}

#[test]
fn jsx_typecheck_off_keeps_tsx_verbatim_passthrough() {
    let case_dir = unique_case_dir("jsx-off");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(&case_dir).unwrap();
    let tsx_path = case_dir.join("Comp.tsx");
    let source = "const Comp = (props: { msg: string }) => <div>{props.msg}</div>;\n";

    // Flag off (the default): the .tsx is mirrored verbatim (React passthrough),
    // its virtual path keeps the `.tsx` extension, and no JSX lowering happens.
    let mut project = VirtualProject::new(&case_dir).unwrap();
    project
        .register_path_with_content(&tsx_path, source)
        .unwrap();
    let virtual_file = project.find_by_original(&tsx_path).unwrap();
    assert_eq!(virtual_file.content.as_str(), source);
    assert_eq!(
        virtual_file.virtual_path,
        project.virtual_root().join("Comp.tsx")
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn jsx_typecheck_on_lowers_tsx_to_plain_ts() {
    let case_dir = unique_case_dir("jsx-on");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(&case_dir).unwrap();
    let tsx_path = case_dir.join("Comp.tsx");
    let source = "const Comp = (props: { msg: string }) => <div class=\"a\">{props.msg}</div>;\n";

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.set_jsx_typecheck(true);
    project
        .register_path_with_content(&tsx_path, source)
        .unwrap();
    let virtual_file = project.find_by_original(&tsx_path).unwrap();

    // Plain `.ts` output: the JSX element is gone, the typed props parameter is
    // verbatim, and the JSX expression is re-emitted as plain TS.
    assert_ts_parses(&virtual_file.content);
    insta::assert_snapshot!(
        "jsx_typecheck_on_lowers_tsx_to_plain_ts",
        virtual_file.content.as_str()
    );
    // Virtual path mirrors to `<name>.ts` so Corsa checks it as TypeScript.
    assert_eq!(
        virtual_file.virtual_path,
        project.virtual_root().join("Comp.tsx.ts")
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn jsx_typecheck_on_types_emits_from_ctx_second_param() {
    // The typed second parameter `{ emit }: Ctx<Emits>` resolves against the
    // injected ambient `Ctx`, and the `emit(...)` call is re-emitted as plain TS
    // so the payload is checked against the declared tuple (#1502, #1497).
    let case_dir = unique_case_dir("jsx-emits");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(&case_dir).unwrap();
    let tsx_path = case_dir.join("Comp.tsx");
    let source = "const Comp = (\n  props: { msg: string },\n  { emit }: Ctx<{ change: [value: number] }>,\n) => <button onClick={() => emit('change', props.msg.length)}>{props.msg}</button>;\n";

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.set_jsx_typecheck(true);
    project
        .register_path_with_content(&tsx_path, source)
        .unwrap();
    let virtual_file = project.find_by_original(&tsx_path).unwrap();

    // Plain TS, and the ambient `Ctx` plus its emit-typing helper are injected so
    // the verbatim `Ctx<{ change: [value: number] }>` annotation resolves.
    assert_ts_parses(&virtual_file.content);
    insta::assert_snapshot!(
        "jsx_typecheck_on_types_emits_from_ctx_second_param",
        virtual_file.content.as_str()
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn jsx_typecheck_on_types_slots_from_ctx_second_param() {
    // `slots` from the typed second parameter is typed as the `Slots` argument,
    // and its usage in a JSX expression is re-emitted so slot access checks.
    let case_dir = unique_case_dir("jsx-slots");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(&case_dir).unwrap();
    let tsx_path = case_dir.join("Comp.tsx");
    let source = "const Comp = (\n  _props: {},\n  { slots }: Ctx<{}, { default: () => unknown }>,\n) => <div>{slots.default()}</div>;\n";

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.set_jsx_typecheck(true);
    project
        .register_path_with_content(&tsx_path, source)
        .unwrap();
    let virtual_file = project.find_by_original(&tsx_path).unwrap();

    assert_ts_parses(&virtual_file.content);
    insta::assert_snapshot!(
        "jsx_typecheck_on_types_slots_from_ctx_second_param",
        virtual_file.content.as_str()
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn jsx_typecheck_on_handles_mixed_props_emits_slots() {
    // A component that uses all three of the typed props, emits, and slots
    // lowers to plain TS that keeps both typed parameters verbatim and re-emits
    // every dynamic JSX expression.
    let case_dir = unique_case_dir("jsx-mixed");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(&case_dir).unwrap();
    let tsx_path = case_dir.join("Comp.tsx");
    let source = "const Comp = (\n  props: { label: string; count?: number },\n  { emit, slots }: Ctx<{ change: [next: number] }, { default: () => unknown }>,\n) => {\n  const next = (props.count ?? 0) + 1;\n  return (\n    <button onClick={() => emit('change', next)}>\n      {props.label}\n      {slots.default()}\n    </button>\n  );\n};\n";

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.set_jsx_typecheck(true);
    project
        .register_path_with_content(&tsx_path, source)
        .unwrap();
    let virtual_file = project.find_by_original(&tsx_path).unwrap();

    assert_ts_parses(&virtual_file.content);
    insta::assert_snapshot!(
        "jsx_typecheck_on_handles_mixed_props_emits_slots",
        virtual_file.content.as_str()
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn jsx_typecheck_on_reemits_v_model_target_as_assignment() {
    // A `v-model` binding target is re-emitted as an assignment to itself so a
    // readonly/const/non-lvalue binding is checked at the binding (#1497). The
    // virtual TS stays plain and parses.
    let case_dir = unique_case_dir("jsx-vmodel");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(&case_dir).unwrap();
    let tsx_path = case_dir.join("Comp.tsx");
    let source = "const Comp = (model: { value: string }) => <input v-model={model.value}/>;\n";

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.set_jsx_typecheck(true);
    project
        .register_path_with_content(&tsx_path, source)
        .unwrap();
    let virtual_file = project.find_by_original(&tsx_path).unwrap();

    assert_ts_parses(&virtual_file.content);
    insta::assert_snapshot!(
        "jsx_typecheck_on_reemits_v_model_target_as_assignment",
        virtual_file.content.as_str()
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn jsx_typecheck_on_binds_v_for_alias_inside_map_callback() {
    // A `v-for` (idiomatic `items.map(…)`) body is re-emitted *inside* the
    // `.map()` callback so its alias binds with the inferred element type — both
    // fixing a spurious "Cannot find name '<alias>'" and checking the body
    // against the real type (#1497).
    let case_dir = unique_case_dir("jsx-vfor");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(&case_dir).unwrap();
    let tsx_path = case_dir.join("Comp.tsx");
    let source = "const Comp = (props: { items: number[] }) => <ul>{props.items.map((item) => <li>{item.toFixed(2)}</li>)}</ul>;\n";

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.set_jsx_typecheck(true);
    project
        .register_path_with_content(&tsx_path, source)
        .unwrap();
    let virtual_file = project.find_by_original(&tsx_path).unwrap();

    assert_ts_parses(&virtual_file.content);
    insta::assert_snapshot!(
        "jsx_typecheck_on_binds_v_for_alias_inside_map_callback",
        virtual_file.content.as_str()
    );

    let _ = fs::remove_dir_all(&case_dir);
}

#[test]
fn jsx_typecheck_on_reemits_style_block_interpolation() {
    // A `<style scoped>` template-literal interpolation (`${props.color}`) is
    // extracted out of the rendered children (#1495) but re-emitted through the
    // sink so it type-checks against the component scope (#1497). The virtual TS
    // stays plain and parses, and the `<style>` element is gone.
    let case_dir = unique_case_dir("jsx-style-expr");
    let _ = fs::remove_dir_all(&case_dir);
    fs::create_dir_all(&case_dir).unwrap();
    let tsx_path = case_dir.join("Comp.tsx");
    let source = "const Comp = (props: { color: string }) => (\n  <>\n    <div class=\"box\">hi</div>\n    <style scoped>{`.box { color: ${props.color}; }`}</style>\n  </>\n);\n";

    let mut project = VirtualProject::new(&case_dir).unwrap();
    project.set_jsx_typecheck(true);
    project
        .register_path_with_content(&tsx_path, source)
        .unwrap();
    let virtual_file = project.find_by_original(&tsx_path).unwrap();

    assert_ts_parses(&virtual_file.content);
    insta::assert_snapshot!(
        "jsx_typecheck_on_reemits_style_block_interpolation",
        virtual_file.content.as_str()
    );

    let _ = fs::remove_dir_all(&case_dir);
}
