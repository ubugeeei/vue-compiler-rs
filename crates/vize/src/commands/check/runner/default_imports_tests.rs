use std::path::{Path, PathBuf};

use vize_s0::path::canonicalize_non_verbatim;

fn unique_case_dir(name: &str) -> PathBuf {
    static NEXT_CASE_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let case_id = NEXT_CASE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("vize-tests")
        .join(format!(
            "check-runner-{name}-{}-{case_id}",
            std::process::id()
        ))
}

fn write(root: &Path, rel: &str, content: &str) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn default_tsconfig_run_reports_transitive_imports_outside_include() {
    let project_root = unique_case_dir("default-transitive-imports");
    let _ = std::fs::remove_dir_all(&project_root);
    std::fs::create_dir_all(project_root.join("inside")).unwrap();
    std::fs::create_dir_all(project_root.join("outside")).unwrap();
    std::fs::write(
        project_root.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "noEmit": true
  },
  "include": ["inside/**/*.ts"]
}"#,
    )
    .unwrap();
    std::fs::write(
        project_root.join("inside/use.ts"),
        r#"import { ITEMS } from '../outside/lib'

export const r = ITEMS.map(({ code, name }) => `${code}:${name}`)
"#,
    )
    .unwrap();
    std::fs::write(
        project_root.join("outside/lib.ts"),
        "export const ITEMS = [{ code: 'en', name: 'English' }, { code: 'ru', name: 'Russian' }]\n",
    )
    .unwrap();

    let mut tsconfig_input_cache = super::TsconfigInputCache::default();
    let mut canonical_paths = super::CanonicalPathCache::default();
    let mut package_routes = vize_canon::PackageRouteResolver::default();
    let collected = super::collect_default_run_files(
        super::DefaultRunFileContext {
            project_root: &project_root,
            cwd: &project_root,
            tsconfig_path: Some(&project_root.join("tsconfig.json")),
            import_options: super::ImportFileOptions::default(),
            check_ignore_set: None,
        },
        &mut tsconfig_input_cache,
        &mut canonical_paths,
        &mut package_routes,
    );
    let files = collected.files;
    let inputs = collected.inputs;
    let reported_files = collected.reported;
    let package_routes = collected.package_routes;

    let included_file = canonicalize_non_verbatim(&project_root.join("inside/use.ts"));
    let transitive_file = canonicalize_non_verbatim(&project_root.join("outside/lib.ts"));

    assert!(files.contains(&included_file));
    assert!(files.contains(&transitive_file));
    assert!(inputs.contains(&included_file));
    assert!(!inputs.contains(&transitive_file));
    assert!(reported_files.contains(&included_file));
    assert!(
        reported_files.contains(&transitive_file),
        "authored imports outside include remain part of the checked program"
    );
    assert!(package_routes.is_empty());

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn default_tsconfig_run_registers_hidden_ambient_declarations_for_type_resolution() {
    let project_root = unique_case_dir("default-hidden-ambient");
    let _ = std::fs::remove_dir_all(&project_root);
    std::fs::create_dir_all(project_root.join(".nuxt/types")).unwrap();
    std::fs::create_dir_all(project_root.join("app/plugins")).unwrap();
    std::fs::write(
        project_root.join("tsconfig.json"),
        r#"{
  "extends": "./.nuxt/tsconfig.json"
}"#,
    )
    .unwrap();
    std::fs::write(
        project_root.join(".nuxt/tsconfig.json"),
        r#"{
  "include": ["../app/**/*.ts", "./nuxt.d.ts"]
}"#,
    )
    .unwrap();
    std::fs::write(
        project_root.join(".nuxt/nuxt.d.ts"),
        "/// <reference path=\"types/import-meta.d.ts\" />\nexport {};\n",
    )
    .unwrap();
    std::fs::write(
        project_root.join(".nuxt/types/import-meta.d.ts"),
        "export {};\ndeclare global { interface ImportMeta { vitest: boolean; } }\n",
    )
    .unwrap();
    std::fs::write(
        project_root.join("app/plugins/auth.ts"),
        "export const runningUnderVitest = import.meta.vitest;\n",
    )
    .unwrap();

    let mut tsconfig_input_cache = super::TsconfigInputCache::default();
    let mut canonical_paths = super::CanonicalPathCache::default();
    let mut package_routes = vize_canon::PackageRouteResolver::default();
    let collected = super::collect_default_run_files(
        super::DefaultRunFileContext {
            project_root: &project_root,
            cwd: &project_root,
            tsconfig_path: Some(&project_root.join("tsconfig.json")),
            import_options: super::ImportFileOptions::default(),
            check_ignore_set: None,
        },
        &mut tsconfig_input_cache,
        &mut canonical_paths,
        &mut package_routes,
    );
    let files = collected.files;
    let inputs = collected.inputs;
    let reported_files = collected.reported;
    let package_routes = collected.package_routes;

    let app_file = canonicalize_non_verbatim(&project_root.join("app/plugins/auth.ts"));
    let ambient_file =
        canonicalize_non_verbatim(&project_root.join(".nuxt/types/import-meta.d.ts"));

    assert!(files.contains(&app_file));
    assert!(files.contains(&ambient_file));
    assert!(inputs.contains(&app_file));
    assert!(!inputs.contains(&ambient_file));
    assert!(reported_files.contains(&app_file));
    assert!(
        !reported_files.contains(&ambient_file),
        "hidden ambient declarations are registered for types, not reported"
    );
    assert!(package_routes.is_empty());

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn explicit_run_registers_discovered_global_component_declaration_imports() {
    let project_root = unique_case_dir("explicit-global-component-ambient-imports");
    let _ = std::fs::remove_dir_all(&project_root);
    let entry = write(
        &project_root,
        "src/App.vue",
        "<template><UiButton /></template>\n",
    );
    let ambient = write(
        &project_root,
        "docs/components.d.ts",
        r#"import "vue";
declare module "vue" {
  export interface GlobalComponents {
    UiButton: typeof import("../packages/ui/UiButton.vue")["default"]
  }
}
export {};
"#,
    );
    let imported = write(&project_root, "packages/ui/UiButton.vue", "<template />\n");
    write(
        &project_root,
        "tsconfig.json",
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "noEmit": true
  },
  "include": ["src/**/*.vue"]
}"#,
    );

    let project_root = project_root.canonicalize().unwrap();
    let mut files = vec![canonicalize_non_verbatim(&entry)];
    let mut tsconfig_input_cache = super::TsconfigInputCache::default();
    let mut canonical_paths = super::CanonicalPathCache::default();
    let mut package_routes = vize_canon::PackageRouteResolver::default();

    let discovered_package_routes = super::register_explicit_ambient_imports(
        &mut files,
        super::ExplicitAmbientImportContext::new(
            &project_root,
            &project_root,
            &project_root.join("tsconfig.json"),
            &project_root,
            &[canonicalize_non_verbatim(&ambient)],
            super::ImportFileOptions::default(),
        ),
        &mut tsconfig_input_cache,
        &mut canonical_paths,
        &mut package_routes,
    );

    let ambient = canonicalize_non_verbatim(&ambient);
    let imported = canonicalize_non_verbatim(&imported);
    assert!(
        files.contains(&ambient),
        "missing ambient declaration: {files:?}"
    );
    assert!(
        files.contains(&imported),
        "missing ambient import: {files:?}"
    );
    assert!(discovered_package_routes.is_empty());

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn explicit_run_discovers_imports_from_non_module_vue_ambient_without_registering_it() {
    let project_root = unique_case_dir("explicit-global-component-non-module-ambient");
    let _ = std::fs::remove_dir_all(&project_root);
    let entry = write(
        &project_root,
        "src/App.vue",
        "<template><UiButton /></template>\n",
    );
    let ambient = write(
        &project_root,
        "src/shims.d.ts",
        r#"declare module "*.css";
declare module "vue" {
  export interface GlobalComponents {
    UiButton: typeof import("../packages/ui/UiButton.vue")["default"]
  }
}
"#,
    );
    let imported = write(&project_root, "packages/ui/UiButton.vue", "<template />\n");
    write(
        &project_root,
        "tsconfig.json",
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "noEmit": true
  },
  "include": ["src/**/*.d.ts", "src/**/*.vue"]
}"#,
    );

    let project_root = project_root.canonicalize().unwrap();
    let mut files = vec![canonicalize_non_verbatim(&entry)];
    let mut tsconfig_input_cache = super::TsconfigInputCache::default();
    let mut canonical_paths = super::CanonicalPathCache::default();
    let mut package_routes = vize_canon::PackageRouteResolver::default();

    let discovered_package_routes = super::register_explicit_ambient_imports(
        &mut files,
        super::ExplicitAmbientImportContext::new(
            &project_root,
            &project_root,
            &project_root.join("tsconfig.json"),
            &project_root,
            &[canonicalize_non_verbatim(&ambient)],
            super::ImportFileOptions::default(),
        ),
        &mut tsconfig_input_cache,
        &mut canonical_paths,
        &mut package_routes,
    );

    let ambient = canonicalize_non_verbatim(&ambient);
    let imported = canonicalize_non_verbatim(&imported);
    assert!(
        !files.contains(&ambient),
        "non-module vue declarations must not become program roots: {files:?}"
    );
    assert!(
        files.contains(&imported),
        "missing global component import: {files:?}"
    );
    assert!(discovered_package_routes.is_empty());

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn append_local_imports_checks_membership_once_per_discovered_path() {
    let root = PathBuf::from("/workspace");
    let existing = root.join("src/existing.ts");
    let added = root.join("src/added.ts");
    let nested = root.join("src/nested.ts");
    let outside = root.join("outside.ts");
    let mut files = vec![existing.clone()];

    let appended = super::append_local_imports(
        &mut files,
        vec![
            existing.clone(),
            added.clone(),
            added.clone(),
            outside,
            nested.clone(),
        ],
        Some(&root.join("src")),
        true,
    );

    assert_eq!(appended, [added.clone(), nested.clone()]);
    assert_eq!(files, [added, existing, nested]);
}
