use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{GlobalComponentStubOptions, collect_project_global_component_stubs};

fn unique_case_dir(name: &str) -> PathBuf {
    static NEXT_CASE_ID: AtomicUsize = AtomicUsize::new(0);

    let case_id = NEXT_CASE_ID.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("vize-tests")
        .join(format!(
            "check-runner-global-components-{name}-{}-{case_id}",
            std::process::id()
        ))
}

#[test]
fn collects_project_global_component_stubs_from_module_declarations() {
    let project_root = unique_case_dir("module-declarations");
    let _ = std::fs::remove_dir_all(&project_root);
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let dts_path = src_dir.join("components.d.mts");
    std::fs::write(
        &dts_path,
        r#"import "vue";
declare module "vue" {
  export interface GlobalComponents {
    ModernComponent: typeof import("./ModernComponent.vue")["default"]
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

    assert_eq!(options.external_template_bindings, ["ModernComponent"]);
    assert!(
        options.auto_import_stubs.iter().any(|stub| {
            stub.contains("declare const ModernComponent:")
                && stub.contains("./src/ModernComponent.vue.ts")
        }),
        "missing ModernComponent stub: {:?}",
        options.auto_import_stubs
    );

    let _ = std::fs::remove_dir_all(&project_root);
}

#[test]
fn explicit_discovery_collects_workspace_global_components_outside_program_files() {
    let project_root = unique_case_dir("workspace-declarations");
    let _ = std::fs::remove_dir_all(&project_root);
    std::fs::create_dir_all(project_root.join("src")).unwrap();
    std::fs::create_dir_all(project_root.join("docs")).unwrap();
    std::fs::create_dir_all(project_root.join("packages/components")).unwrap();
    std::fs::create_dir_all(project_root.join("node_modules/package")).unwrap();
    std::fs::write(project_root.join("src/UserComponent.vue"), "<template />").unwrap();
    std::fs::write(
        project_root.join("docs/components.d.ts"),
        r#"import "vue";
declare module "vue" {
  export interface GlobalComponents {
    OutsideComponent: typeof import("../packages/components/OutsideComponent.vue")["default"]
  }
}
export {};
"#,
    )
    .unwrap();
    std::fs::write(
        project_root.join("node_modules/package/components.d.ts"),
        r#"declare module "vue" {
  export interface GlobalComponents {
    IgnoredDependencyComponent: unknown
  }
}
"#,
    )
    .unwrap();

    let mut options = vize_canon::virtual_ts::VirtualTsOptions::default();
    collect_project_global_component_stubs(
        &mut options,
        &[project_root.join("src/UserComponent.vue")],
        &project_root,
        None,
        GlobalComponentStubOptions {
            discover_workspace_declarations: true,
        },
    );

    assert_eq!(options.external_template_bindings, ["OutsideComponent"]);
    assert!(
        options.auto_import_stubs.iter().any(|stub| {
            stub.contains("declare const OutsideComponent:")
                && stub.contains("./packages/components/OutsideComponent.vue.ts")
        }),
        "missing OutsideComponent stub: {:?}",
        options.auto_import_stubs
    );
    assert!(
        options
            .auto_import_stubs
            .iter()
            .all(|stub| { !stub.contains("IgnoredDependencyComponent") }),
        "dependency declarations must stay excluded: {:?}",
        options.auto_import_stubs
    );

    let _ = std::fs::remove_dir_all(&project_root);
}
