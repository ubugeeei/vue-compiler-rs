use std::path::Path;

use vize_s0::{String, cstr};

pub(super) fn rewrite_global_component_imports_for_virtual_project(
    type_annotation: &str,
    project_root: &Path,
) -> String {
    let bytes = type_annotation.as_bytes();
    let mut out = String::with_capacity(type_annotation.len());
    let mut i = 0usize;

    while i < bytes.len() {
        let quote = if type_annotation[i..].starts_with("import('") {
            Some('\'')
        } else if type_annotation[i..].starts_with("import(\"") {
            Some('"')
        } else {
            None
        };

        let Some(quote) = quote else {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        };

        out.push_str("import(");
        out.push(quote);
        i += 8;

        let start = i;
        while i < bytes.len() && bytes[i] != quote as u8 {
            i += 1;
        }

        let specifier = &type_annotation[start..i];
        out.push_str(&virtual_project_global_component_specifier(
            specifier,
            project_root,
        ));

        if i < bytes.len() {
            out.push(quote);
            i += 1;
        }
    }

    out
}

fn virtual_project_global_component_specifier(specifier: &str, project_root: &Path) -> String {
    if !specifier.ends_with(".vue") {
        return specifier.into();
    }

    let specifier_path = Path::new(specifier);
    if let Some(relative) = specifier_path
        .is_absolute()
        .then(|| specifier_path.strip_prefix(project_root).ok())
        .flatten()
    {
        let mut rendered = cstr!("./{}", relative.display());
        rendered.push_str(".ts");
        return rendered;
    }

    cstr!("{specifier}.ts")
}
