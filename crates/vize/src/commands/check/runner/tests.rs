use super::{
    GlobalComponentStubOptions, collect_project_global_component_stubs, dialect_from_features,
    find_nearest_tsconfig_dir, is_suppressed_false_positive, resolve_declaration_dir,
    resolve_declaration_emit_options, resolve_project_root, resolve_tsconfig_path,
    validate_corsa_server_count,
};
use crate::commands::check::tsconfig_inputs::TsconfigDeclarationOptions;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

fn unique_case_dir(name: &str) -> PathBuf {
    static NEXT_CASE_ID: AtomicUsize = AtomicUsize::new(0);
    let case_id = NEXT_CASE_ID.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("vize-tests")
        .join(format!(
            "check-runner-{name}-{}-{case_id}",
            std::process::id()
        ))
}

mod nuxt_root;
mod retained_files;
#[test]
fn suppresses_nuxt_nitro_import_meta_conflict_false_positive() {
    let diagnostic = vize_canon::BatchDiagnostic {
        file: PathBuf::from("app/app.vue"),
        line: 0,
        column: 0,
        message: "Interface 'ImportMeta' cannot simultaneously extend types 'NitroStaticBuildFlags' and 'NitroImportMeta'.\nNamed property 'preset' of types 'NitroStaticBuildFlags' and 'NitroImportMeta' are not identical.".into(),
        code: Some(2320),
        severity: 1,
        block_type: None,
    };

    assert!(is_suppressed_false_positive(&diagnostic));

    let mut unrelated = diagnostic.clone();
    unrelated.message = "Interface 'Other' cannot simultaneously extend types".into();
    assert!(!is_suppressed_false_positive(&unrelated));
}

#[test]
fn collects_project_global_component_stubs_from_ambient_dts() {
    let project_root = unique_case_dir("global-components");
    let _ = std::fs::remove_dir_all(&project_root);
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let dts_path = src_dir.join("components.d.ts");
    std::fs::write(
        &dts_path,
        r#"import "vue";
declare module "vue" {
  export interface GlobalComponents {
    GlobalComponent: typeof import("./GlobalComponent.vue")["default"]
  }
}
export {};
"#,
    )
    .unwrap();

    let mut options = vize_canon::virtual_ts::VirtualTsOptions::default();
    collect_project_global_component_stubs(
        &mut options,
        std::slice::from_ref(&dts_path),
        &project_root,
        None,
        GlobalComponentStubOptions::default(),
    );

    assert_eq!(options.external_template_bindings, ["GlobalComponent"]);
    assert!(
        options.auto_import_stubs.iter().any(|stub| {
            stub.contains("declare const GlobalComponent:")
                && stub.contains("./src/GlobalComponent.vue.ts")
        }),
        "missing GlobalComponent stub: {:?}",
        options.auto_import_stubs
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn resolves_monorepo_root_for_files_spanning_package_tsconfigs() {
    let project_root = unique_case_dir("monorepo-root");
    let _ = std::fs::remove_dir_all(&project_root);
    let app_dir = project_root.join("packages/app");
    let ui_dir = project_root.join("packages/ui");
    std::fs::create_dir_all(app_dir.join("src")).unwrap();
    std::fs::create_dir_all(ui_dir.join("src")).unwrap();
    std::fs::write(project_root.join("tsconfig.json"), "{}").unwrap();
    std::fs::write(app_dir.join("tsconfig.json"), "{}").unwrap();
    std::fs::write(ui_dir.join("tsconfig.json"), "{}").unwrap();
    let files = vec![app_dir.join("src/App.vue"), ui_dir.join("src/UiButton.vue")];
    for file in &files {
        std::fs::write(file, "<template />").unwrap();
    }

    let resolved_root = resolve_project_root(None, &project_root, &files);
    let resolved_tsconfig = resolve_tsconfig_path(None, &project_root, &resolved_root, &files);

    assert_eq!(resolved_root, project_root);
    assert_eq!(resolved_tsconfig, Some(resolved_root.join("tsconfig.json")));

    let _ = std::fs::remove_dir_all(&resolved_root);
}

#[test]
fn resolves_package_root_for_relative_tsconfig_inside_one_package() {
    let project_root = unique_case_dir("package-root");
    let _ = std::fs::remove_dir_all(&project_root);
    let app_dir = project_root.join("packages/app");
    std::fs::create_dir_all(app_dir.join("src")).unwrap();
    std::fs::write(project_root.join("tsconfig.json"), "{}").unwrap();
    std::fs::write(app_dir.join("tsconfig.json"), "{}").unwrap();
    let files = vec![app_dir.join("src/App.vue"), app_dir.join("src/main.ts")];
    for file in &files {
        std::fs::write(file, "").unwrap();
    }
    let tsconfig = Path::new("tsconfig.json");
    let resolved_root = resolve_project_root(Some(tsconfig), &app_dir, &files);
    let resolved_tsconfig = resolve_tsconfig_path(Some(tsconfig), &app_dir, &resolved_root, &files);
    assert_eq!(resolved_root, app_dir);
    assert_eq!(resolved_tsconfig, Some(resolved_root.join("tsconfig.json")));

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn resolves_common_root_when_explicit_tsconfig_is_below_inputs() {
    let project_root = unique_case_dir("explicit-tsconfig-below-inputs");
    let _ = std::fs::remove_dir_all(&project_root);
    let config_dir = project_root.join("config");
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&src_dir).unwrap();
    let tsconfig = config_dir.join("tsconfig.json");
    let app = src_dir.join("App.vue");
    std::fs::write(&tsconfig, "{}").unwrap();
    std::fs::write(&app, "<template />").unwrap();
    let files = vec![app];

    let resolved_root = resolve_project_root(Some(&tsconfig), &project_root, &files);
    let resolved_tsconfig =
        resolve_tsconfig_path(Some(&tsconfig), &project_root, &resolved_root, &files);

    assert_eq!(resolved_root, project_root);
    assert_eq!(resolved_tsconfig, Some(tsconfig));

    let _ = std::fs::remove_dir_all(&resolved_root);
}

#[test]
fn explicit_tsconfig_root_ignores_node_modules_dependency_roots() {
    let project_root = unique_case_dir("explicit-tsconfig-node-modules");
    let _ = std::fs::remove_dir_all(&project_root);
    let app_dir = project_root.join("packages/app");
    let src_dir = app_dir.join("src");
    let dep_dir = project_root.join("node_modules/.pnpm/vue/node_modules/vue");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dep_dir).unwrap();
    let tsconfig = app_dir.join("tsconfig.json");
    let app = src_dir.join("App.vue");
    let dep = dep_dir.join("jsx.d.ts");
    std::fs::write(&tsconfig, "{}").unwrap();
    std::fs::write(&app, "<template />").unwrap();
    std::fs::write(&dep, "export {};\n").unwrap();
    let files = vec![app, dep];

    let resolved_root = resolve_project_root(Some(&tsconfig), &app_dir, &files);

    assert_eq!(resolved_root, app_dir);

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn falls_back_to_cwd_resolution_when_files_have_no_tsconfig() {
    let project_root = unique_case_dir("no-tsconfig");
    let _ = std::fs::remove_dir_all(&project_root);
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let files = vec![src_dir.join("App.vue")];
    std::fs::write(&files[0], "<template />").unwrap();

    let resolved_root = resolve_project_root(None, &project_root, &files);
    let resolved_tsconfig = resolve_tsconfig_path(None, &project_root, &resolved_root, &files);
    let expected_root =
        find_nearest_tsconfig_dir(&project_root).unwrap_or_else(|| project_root.clone());

    assert_eq!(resolved_root, expected_root);
    assert_eq!(
        resolved_tsconfig,
        resolved_root
            .join("tsconfig.json")
            .exists()
            .then_some(resolved_root.join("tsconfig.json"))
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn falls_back_to_common_file_parent_for_external_files_without_tsconfig() {
    let case_root = std::env::temp_dir().join(format!(
        "vize-check-runner-external-root-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&case_root);
    let cwd = case_root.join("cwd");
    let source_dir = case_root.join("external");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&source_dir).unwrap();
    let files = vec![source_dir.join("Repro.vue")];
    std::fs::write(&files[0], "<template />").unwrap();

    let resolved_root = resolve_project_root(None, &cwd, &files);
    let resolved_tsconfig = resolve_tsconfig_path(None, &cwd, &resolved_root, &files);

    assert_eq!(resolved_root, source_dir);
    assert_eq!(resolved_tsconfig, None);

    let _ = std::fs::remove_dir_all(&case_root);
}

#[test]
fn resolve_declaration_dir_defaults_to_dist_types() {
    let project_root = PathBuf::from("/workspace/project");
    let tsconfig_options = TsconfigDeclarationOptions::default();
    assert_eq!(
        resolve_declaration_dir(None, &tsconfig_options, &project_root),
        project_root.join("dist").join("types")
    );
    assert_eq!(
        resolve_declaration_dir(Some(Path::new("types")), &tsconfig_options, &project_root),
        project_root.join("types")
    );
}

#[test]
fn resolve_declaration_dir_uses_tsconfig_when_cli_dir_is_absent() {
    let project_root = PathBuf::from("/workspace/project");
    let tsconfig_options = TsconfigDeclarationOptions {
        declaration_dir: Some(project_root.join("types")),
        out_dir: Some(project_root.join("dist")),
        declaration_map: Some(true),
    };

    assert_eq!(
        resolve_declaration_dir(None, &tsconfig_options, &project_root),
        project_root.join("types")
    );
    assert_eq!(
        resolve_declaration_dir(Some(Path::new("custom")), &tsconfig_options, &project_root),
        project_root.join("custom")
    );

    let out_dir_only = TsconfigDeclarationOptions {
        declaration_dir: None,
        out_dir: Some(project_root.join("dist")),
        declaration_map: None,
    };
    assert_eq!(
        resolve_declaration_dir(None, &out_dir_only, &project_root),
        project_root.join("dist")
    );
}

#[test]
fn resolve_declaration_emit_options_uses_tsconfig_declaration_map() {
    let project_root = unique_case_dir("declaration-options");
    let _ = std::fs::remove_dir_all(&project_root);
    std::fs::create_dir_all(&project_root).unwrap();
    std::fs::write(
        project_root.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "declarationDir": "types",
    "declarationMap": true
  }
}"#,
    )
    .unwrap();

    let options = resolve_declaration_emit_options(
        None,
        Some(&project_root.join("tsconfig.json")),
        &project_root,
    );

    assert_eq!(options.out_dir, project_root.join("types"));
    assert!(options.declaration_map);

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn resolves_configured_vue_dialect_for_canon_generation() {
    use crate::config::VueVersion;

    // Plumbing for issue #1392: `vue.version` from config reaches canon's
    // virtual-TS generation. An explicit Vue 2 config selects V2; an unset
    // `vue.version` defaults to Vue 3 so the default path stays unchanged.
    assert_eq!(dialect_from_features(Some(VueVersion::V2)), VueVersion::V2);
    assert_eq!(
        dialect_from_features(Some(VueVersion::V2_7)),
        VueVersion::V2_7
    );
    assert_eq!(dialect_from_features(None), VueVersion::V3);
    assert_eq!(dialect_from_features(Some(VueVersion::V3)), VueVersion::V3);
}

#[test]
fn validates_corsa_server_counts() {
    assert!(validate_corsa_server_count(None).is_ok());
    assert!(validate_corsa_server_count(Some(1)).is_ok());
    assert!(validate_corsa_server_count(Some(2)).is_ok());
    assert!(validate_corsa_server_count(Some(32)).is_ok());
    assert!(validate_corsa_server_count(Some(0)).is_err());
    assert!(validate_corsa_server_count(Some(33)).is_err());
}
