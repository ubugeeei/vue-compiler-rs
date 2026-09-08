use oxc_span::SourceType;
use tempfile::TempDir;

use super::{
    AUTHORED_VUE_TS_ALIAS_SENTINEL, AUTHORED_VUE_TS_SENTINEL, import_rewriter::ImportRewriter,
    virtual_specifier_message::MISSING_VUE_IMPORT_SENTINEL,
};

#[test]
fn authored_vue_ts_specifiers_cannot_resolve_generated_mirrors() {
    let case = TempDir::new().expect("temp project");
    let project = std::fs::canonicalize(case.path()).expect("canonical temp project");
    let source_dir = project.join("src");
    let virtual_root = crate::batch::project_virtual_root(&project);
    std::fs::create_dir_all(&source_dir).expect("source directory");
    std::fs::write(source_dir.join("Mirror.vue"), "<template />").expect("SFC mirror");
    std::fs::write(source_dir.join("MirrorJsx.vue"), "<template />").expect("TSX SFC mirror");
    std::fs::write(source_dir.join("Real.vue.ts"), "export const real = true")
        .expect("real authored vue.ts");
    std::fs::write(
        source_dir.join("RealJsx.vue.tsx"),
        "export const real = true",
    )
    .expect("real authored vue.tsx");
    std::fs::create_dir_all(source_dir.join("Directory.vue.ts")).expect("directory module");
    std::fs::write(
        source_dir.join("Directory.vue.ts/index.ts"),
        "export const real = true",
    )
    .expect("directory module index");
    for path in [
        "Declaration.vue.d.ts",
        "Runtime.vue.js",
        "FullDeclaration.vue.ts.d.ts",
        "FullSource.vue.ts.ts",
    ] {
        std::fs::write(source_dir.join(path), "export const real = true")
            .expect("extension-substituted module");
    }
    for path in [
        "Real.vue",
        "RealJsx.vue",
        "Directory.vue",
        "Declaration.vue",
        "Runtime.vue",
        "FullDeclaration.vue",
        "FullSource.vue",
    ] {
        std::fs::write(source_dir.join(path), "<template />").expect("colliding SFC mirror");
    }
    std::fs::write(source_dir.join("Generated.vue"), "<template />").expect("generated SFC");

    let absent_absolute = project.join("absent/Absolute.vue.ts");
    std::fs::create_dir_all(absent_absolute.parent().unwrap()).expect("absolute source directory");
    std::fs::write(project.join("absent/Absolute.vue"), "<template />")
        .expect("absolute SFC mirror");
    let absent_absolute = absent_absolute.to_string_lossy().replace('\\', "/");
    let source = format!(
        "import './Mirror.vue.ts';\n\
         import './MirrorJsx.vue.tsx';\n\
         import './Real.vue.ts';\n\
         import './RealJsx.vue.tsx';\n\
         import './Directory.vue.ts';\n\
         import './Declaration.vue.ts';\n\
         import './Runtime.vue.ts';\n\
         import './FullDeclaration.vue.ts';\n\
         import './FullSource.vue.ts';\n\
         import '@/Alias.vue.ts';\n\
         import '{absent_absolute}';\n\
         import './Generated.vue';\n"
    );
    let expected = format!(
        "import './Mirror.vue.ts{AUTHORED_VUE_TS_SENTINEL}';\n\
         import './MirrorJsx.vue.tsx{AUTHORED_VUE_TS_SENTINEL}';\n\
         import './Real.vue.ts{AUTHORED_VUE_TS_ALIAS_SENTINEL}';\n\
         import './RealJsx.vue.tsx{AUTHORED_VUE_TS_ALIAS_SENTINEL}';\n\
         import './Directory.vue.ts{AUTHORED_VUE_TS_ALIAS_SENTINEL}';\n\
         import './Declaration.vue.ts{AUTHORED_VUE_TS_ALIAS_SENTINEL}';\n\
         import './Runtime.vue.ts{AUTHORED_VUE_TS_ALIAS_SENTINEL}';\n\
         import './FullDeclaration.vue.ts{AUTHORED_VUE_TS_ALIAS_SENTINEL}';\n\
         import './FullSource.vue.ts{AUTHORED_VUE_TS_ALIAS_SENTINEL}';\n\
         import '@/Alias.vue.ts';\n\
         import '{absent_absolute}{AUTHORED_VUE_TS_SENTINEL}';\n\
         import './Generated.vue.ts';\n"
    );
    let rewriter = ImportRewriter::new();
    assert_eq!(
        rewriter
            .rewrite(&source, SourceType::ts(), Some(&source_dir))
            .code,
        expected
    );
    assert_eq!(
        rewriter
            .rewrite_for_virtual_project(
                &source,
                SourceType::ts(),
                (&project, &virtual_root),
                Some(&source_dir),
            )
            .code,
        expected
    );

    // Callers without an authored directory cannot prove that a relative
    // `.vue.ts` is the generated mirror rather than a real source file.
    assert_eq!(
        rewriter
            .rewrite("import './Mirror.vue.ts';", SourceType::ts(), None,)
            .code,
        "import './Mirror.vue.ts';"
    );
}

#[test]
fn missing_vue_specifiers_keep_ts2307_reachable() {
    let case = TempDir::new().expect("temp project");
    let source_dir = case.path().join("src");
    std::fs::create_dir_all(&source_dir).expect("source directory");
    std::fs::write(source_dir.join("Present.vue"), "<template />").expect("present SFC");

    let source = "import './Present.vue';\nimport './Missing.vue';\n";
    let rewriter = ImportRewriter::new();

    assert_eq!(
        rewriter
            .rewrite(source, SourceType::ts(), Some(&source_dir))
            .code,
        format!(
            "import './Present.vue.ts';\nimport './Missing.vue.ts{MISSING_VUE_IMPORT_SENTINEL}';\n"
        )
    );
    assert_eq!(
        rewriter.rewrite(source, SourceType::ts(), None).code,
        "import './Present.vue.ts';\nimport './Missing.vue.ts';\n"
    );
}
