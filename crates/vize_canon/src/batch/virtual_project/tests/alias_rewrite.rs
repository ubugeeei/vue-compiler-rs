use std::fs;

use super::{VirtualProject, unique_case_dir};

fn write_vue_import_case(case: &std::path::Path, tsconfig: Option<&str>) -> std::path::PathBuf {
    let _ = fs::remove_dir_all(case);
    fs::create_dir_all(case.join("src/components/api")).unwrap();
    if let Some(tsconfig) = tsconfig {
        fs::write(case.join("tsconfig.json"), tsconfig).unwrap();
    }
    fs::write(
        case.join("src/components/api/DirectiveTable.vue"),
        "<script setup lang=\"ts\">export interface Props { id: string }</script>",
    )
    .unwrap();
    let app = case.join("src/App.vue");
    fs::write(
        &app,
        r#"<script setup lang="ts">
import DirectiveTable from "@/components/api/DirectiveTable.vue";
void DirectiveTable;
</script>
"#,
    )
    .unwrap();
    app
}

#[test]
fn batch_sfc_keeps_unconfigured_alias_vue_import_authored() {
    let case = unique_case_dir("unconfigured-alias-vue-import");
    let app = write_vue_import_case(&case, None);

    let mut project = VirtualProject::new(&case).unwrap();
    project.register_path(&app).unwrap();
    let generated = project.find_by_original(&app).unwrap().content.as_str();

    assert!(generated.contains("\"@/components/api/DirectiveTable.vue\""));
    assert!(!generated.contains("\"@/components/api/DirectiveTable.vue.ts\""));

    let _ = fs::remove_dir_all(&case);
}

#[test]
fn batch_sfc_rewrites_configured_alias_vue_import_to_the_mirror() {
    let case = unique_case_dir("configured-alias-vue-import");
    let app = write_vue_import_case(
        &case,
        Some(
            r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"]
    }
  }
}"#,
        ),
    );

    let mut project = VirtualProject::new(&case).unwrap();
    project.register_path(&app).unwrap();
    let generated = project.find_by_original(&app).unwrap().content.as_str();

    assert!(generated.contains("\"@/components/api/DirectiveTable.vue.ts\""));

    let _ = fs::remove_dir_all(&case);
}

#[test]
fn batch_sfc_keeps_vue_suffixed_alias_key_import_authored() {
    let case = unique_case_dir("vue-suffixed-alias-key-import");
    let app = write_vue_import_case(
        &case,
        Some(
            r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*.vue": ["src/*.vue"]
    }
  }
}"#,
        ),
    );

    let mut project = VirtualProject::new(&case).unwrap();
    project.register_path(&app).unwrap();
    let generated = project.find_by_original(&app).unwrap().content.as_str();

    assert!(generated.contains("\"@/components/api/DirectiveTable.vue\""));
    assert!(!generated.contains("\"@/components/api/DirectiveTable.vue.ts\""));

    let _ = fs::remove_dir_all(&case);
}
