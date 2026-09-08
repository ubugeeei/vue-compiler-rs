//! When Nuxt's generated declaration graph becomes the authority for
//! `$`-prefixed template globals, and when the permissive stand-ins stay.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use vize_canon::virtual_ts::VirtualTsOptions;

use super::{detect_legacy_nuxt_auto_imports, detect_nuxt_auto_imports};

fn unique_case_dir(name: &str) -> PathBuf {
    static NEXT_CASE_ID: AtomicUsize = AtomicUsize::new(0);

    let case_id = NEXT_CASE_ID.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("vize-tests")
        .join(format!(
            "check-nuxt-strict-{name}-{}-{case_id}",
            std::process::id()
        ))
}

fn write_nuxt_project(project_root: &PathBuf) {
    let _ = std::fs::remove_dir_all(project_root);
    std::fs::create_dir_all(project_root.join(".nuxt/types")).unwrap();
    std::fs::write(
        project_root.join("nuxt.config.ts"),
        "export default defineNuxtConfig({ modules: ['@nuxtjs/i18n'] })",
    )
    .unwrap();
}

const GENERATED_IMPORTS: &str = r#"declare global {
  const useI18n: () => { t: (key: string) => string }
}
export {}
"#;

const GENERATED_ROUTER_IMPORTS: &str = r#"declare global {
  const useRoute: () => { path: string; query: Record<string, string | undefined> }
  const useRouter: () => { push(to: string): Promise<void> }
}
export {}
"#;

const GENERATED_CUSTOM_PROPERTIES: &str = r#"declare module 'vue' {
  export interface ComponentCustomProperties {
    $shout: (label: string) => string
  }
}
export {}
"#;

const GENERATED_GLOBAL_I18N_VALUES: &str = r#"import type { Composer } from 'vue-i18n'

declare global {
  var $t: (Composer)['t']
  var $rt: (Composer)['rt']
}

export {}
"#;

fn global_names(options: &VirtualTsOptions) -> Vec<&str> {
    options
        .template_globals
        .iter()
        .map(|global| global.name.as_str())
        .collect()
}

#[test]
fn generated_imports_make_the_declaration_graph_authoritative() {
    let project_root = unique_case_dir("generated-imports");
    write_nuxt_project(&project_root);
    std::fs::write(project_root.join(".nuxt/imports.d.ts"), GENERATED_IMPORTS).unwrap();
    std::fs::write(
        project_root.join(".nuxt/types/plugins.d.ts"),
        GENERATED_CUSTOM_PROPERTIES,
    )
    .unwrap();

    let mut options = VirtualTsOptions::default();
    let _ = detect_nuxt_auto_imports(&mut options, &project_root);

    // `$shout` is declared by the generated graph and keeps its declaration.
    // The vue-i18n instance surface an auto-imported `useI18n` used to imply is
    // gone: nothing here declares `$t`, so it resolves on the component
    // instance and an authored use reports the way the Vue toolchain does.
    assert!(options.strict_instance_globals);
    assert_eq!(global_names(&options), vec!["$shout"]);

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn generated_router_auto_imports_expose_template_route_globals() {
    let project_root = unique_case_dir("generated-router-globals");
    write_nuxt_project(&project_root);
    std::fs::write(
        project_root.join(".nuxt/imports.d.ts"),
        GENERATED_ROUTER_IMPORTS,
    )
    .unwrap();

    let mut options = VirtualTsOptions::default();
    let _ = detect_nuxt_auto_imports(&mut options, &project_root);

    assert!(options.strict_instance_globals);
    let globals = options
        .template_globals
        .iter()
        .map(|global| (global.name.as_str(), global.type_annotation.as_str()))
        .collect::<Vec<_>>();
    assert!(
        globals.contains(&("$route", "ReturnType<typeof useRoute>")),
        "expected typed $route template global, got: {globals:#?}"
    );
    assert!(
        globals.contains(&("$router", "ReturnType<typeof useRouter>")),
        "expected typed $router template global, got: {globals:#?}"
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn generated_global_dollar_values_are_template_bindings() {
    let project_root = unique_case_dir("generated-global-dollar-values");
    write_nuxt_project(&project_root);
    std::fs::write(project_root.join(".nuxt/imports.d.ts"), GENERATED_IMPORTS).unwrap();
    std::fs::write(
        project_root.join(".nuxt/types/i18n-plugin.d.ts"),
        GENERATED_GLOBAL_I18N_VALUES,
    )
    .unwrap();

    let mut options = VirtualTsOptions::default();
    let _ = detect_nuxt_auto_imports(&mut options, &project_root);

    assert!(options.strict_instance_globals);
    assert!(!global_names(&options).contains(&"$t"));
    assert!(
        options
            .external_template_bindings
            .iter()
            .any(|name| name == "$t")
    );
    assert!(
        options
            .external_template_bindings
            .iter()
            .any(|name| name == "$rt")
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn missing_generated_imports_keep_the_permissive_template_globals() {
    let project_root = unique_case_dir("no-generated-imports");
    write_nuxt_project(&project_root);
    // Declarations without an auto-import manifest: the project never ran a
    // full type generation, so it stays in the documented degraded mode where
    // auto-imports and template globals fall back to permissive stand-ins.
    std::fs::write(
        project_root.join(".nuxt/types/plugins.d.ts"),
        GENERATED_CUSTOM_PROPERTIES,
    )
    .unwrap();

    let mut options = VirtualTsOptions::default();
    let _ = detect_nuxt_auto_imports(&mut options, &project_root);

    assert!(!options.strict_instance_globals);
    let mut names = global_names(&options);
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["$d", "$i18n", "$n", "$rt", "$shout", "$t", "$te", "$tm"]
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn a_project_without_generated_types_keeps_the_permissive_template_globals() {
    let project_root = unique_case_dir("no-generated-types");
    let _ = std::fs::remove_dir_all(&project_root);
    std::fs::create_dir_all(&project_root).unwrap();
    std::fs::write(
        project_root.join("nuxt.config.ts"),
        "export default defineNuxtConfig({ modules: ['@nuxtjs/i18n'] })",
    )
    .unwrap();

    let mut options = VirtualTsOptions::default();
    let _ = detect_nuxt_auto_imports(&mut options, &project_root);

    assert!(!options.strict_instance_globals);
    assert!(global_names(&options).contains(&"$t"));

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn the_legacy_vue2_dialect_keeps_the_permissive_template_globals() {
    let project_root = unique_case_dir("legacy-vue2");
    write_nuxt_project(&project_root);
    std::fs::write(project_root.join(".nuxt/imports.d.ts"), GENERATED_IMPORTS).unwrap();

    let mut options = VirtualTsOptions::default();
    let _ = detect_legacy_nuxt_auto_imports(&mut options, &project_root);

    // The Vue 2 template context is a structural fallback that carries almost
    // none of the real instance surface, so resolving names on it would invent
    // diagnostics rather than match the toolchain.
    assert!(!options.strict_instance_globals);
    assert!(global_names(&options).contains(&"$route"));

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn a_non_nuxt_project_never_turns_on_the_strict_form() {
    let project_root = unique_case_dir("not-nuxt");
    let _ = std::fs::remove_dir_all(&project_root);
    std::fs::create_dir_all(project_root.join(".nuxt")).unwrap();
    std::fs::write(project_root.join(".nuxt/imports.d.ts"), GENERATED_IMPORTS).unwrap();

    let mut options = VirtualTsOptions::default();
    let _ = detect_nuxt_auto_imports(&mut options, &project_root);

    assert!(!options.strict_instance_globals);

    let _ = std::fs::remove_dir_all(&project_root);
}
