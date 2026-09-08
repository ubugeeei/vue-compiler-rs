use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Declaration, Expression, ImportDeclarationSpecifier, Statement,
};
use oxc_parser::Parser;
use oxc_span::SourceType;
use vize_carton::{CompactString, FxHashMap, FxHashSet, ToCompactString};

#[derive(Clone)]
pub(super) struct RuntimeImport {
    pub(super) source: CompactString,
    pub(super) imported: CompactString,
}

pub(super) type RuntimePropVisitSet = FxHashSet<CompactString>;

pub(super) fn find_imported_runtime_binding(source: &str, local: &str) -> Option<RuntimeImport> {
    parse_ts(source, "script.ts", |program| {
        for stmt in &program.body {
            let Statement::ImportDeclaration(import_decl) = stmt else {
                continue;
            };
            let Some(specifiers) = import_decl.specifiers.as_ref() else {
                continue;
            };
            for specifier in specifiers {
                match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(specifier)
                        if specifier.local.name.as_str() == local =>
                    {
                        return Some(RuntimeImport {
                            source: import_decl.source.value.to_compact_string(),
                            imported: specifier.imported.name().as_str().to_compact_string(),
                        });
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier)
                        if specifier.local.name.as_str() == local =>
                    {
                        return Some(RuntimeImport {
                            source: import_decl.source.value.to_compact_string(),
                            imported: "default".to_compact_string(),
                        });
                    }
                    _ => {}
                }
            }
        }
        None
    })
    .flatten()
}

pub(super) fn collect_imports<'a>(
    program: &'a oxc_ast::ast::Program<'a>,
) -> FxHashMap<&'a str, RuntimeImport> {
    let mut imports = FxHashMap::default();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import_decl) = stmt else {
            continue;
        };
        let Some(specifiers) = import_decl.specifiers.as_ref() else {
            continue;
        };
        for specifier in specifiers {
            match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                    imports.insert(
                        specifier.local.name.as_str(),
                        RuntimeImport {
                            source: import_decl.source.value.to_compact_string(),
                            imported: specifier.imported.name().as_str().to_compact_string(),
                        },
                    );
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                    imports.insert(
                        specifier.local.name.as_str(),
                        RuntimeImport {
                            source: import_decl.source.value.to_compact_string(),
                            imported: "default".to_compact_string(),
                        },
                    );
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => {}
            }
        }
    }
    imports
}

pub(super) fn collect_top_level_values<'a>(
    program: &'a oxc_ast::ast::Program<'a>,
) -> FxHashMap<&'a str, &'a Expression<'a>> {
    let mut values = FxHashMap::default();
    for stmt in &program.body {
        let declaration = match stmt {
            Statement::VariableDeclaration(variable) => Some(variable.as_ref()),
            Statement::ExportNamedDeclaration(export_decl) => {
                match export_decl.declaration.as_ref() {
                    Some(Declaration::VariableDeclaration(variable)) => Some(variable.as_ref()),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(declaration) = declaration else {
            continue;
        };
        for declarator in &declaration.declarations {
            let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };
            if let Some(init) = declarator.init.as_ref() {
                values.insert(id.name.as_str(), init);
            }
        }
    }
    values
}

pub(super) fn resolve_runtime_import_path(current_file: &Path, specifier: &str) -> Option<PathBuf> {
    if specifier.starts_with('#') || specifier.starts_with("node:") {
        return None;
    }

    let candidate = if let Some(rest) = specifier.strip_prefix("@/") {
        let src = current_file
            .parent()?
            .ancestors()
            .find(|path| path.file_name().is_some_and(|name| name == "src"))?;
        src.join(rest)
    } else if specifier.starts_with('/') {
        PathBuf::from(specifier)
    } else if specifier.starts_with('.') {
        current_file.parent()?.join(specifier)
    } else {
        return resolve_workspace_package_runtime_import_path(current_file, specifier);
    };

    resolve_runtime_candidate(candidate)
}

fn resolve_workspace_package_runtime_import_path(
    current_file: &Path,
    specifier: &str,
) -> Option<PathBuf> {
    let package_path = workspace_package_subpath(specifier)?;
    for ancestor in current_file.parent()?.ancestors() {
        let packages = ancestor.join("packages");
        if !packages.is_dir() {
            continue;
        }
        if let Some(path) = resolve_runtime_candidate(packages.join(&package_path)) {
            return Some(path);
        }
    }
    None
}

fn workspace_package_subpath(specifier: &str) -> Option<PathBuf> {
    let mut parts = specifier.split('/').filter(|part| !part.is_empty());
    let first = parts.next()?;
    if first.starts_with('@') {
        let package = parts.next()?;
        let mut path = PathBuf::from(package);
        for part in parts {
            path.push(part);
        }
        Some(path)
    } else {
        let mut path = PathBuf::from(first);
        for part in parts {
            path.push(part);
        }
        Some(path)
    }
}

fn resolve_runtime_candidate(candidate: PathBuf) -> Option<PathBuf> {
    if let Some(path) = resolve_ts_source_path_for_js_specifier(&candidate) {
        return Some(path);
    }
    if candidate.is_file() {
        return Some(candidate.canonicalize().unwrap_or(candidate));
    }
    for extension in ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "vue"] {
        let path = candidate.with_extension(extension);
        if path.is_file() {
            return Some(path.canonicalize().unwrap_or(path));
        }
    }
    if candidate.is_dir() {
        for index in [
            "index.ts",
            "index.tsx",
            "index.mts",
            "index.cts",
            "index.js",
            "index.jsx",
            "index.mjs",
            "index.cjs",
        ] {
            let path = candidate.join(index);
            if path.is_file() {
                return Some(path.canonicalize().unwrap_or(path));
            }
        }
    }
    None
}

fn resolve_ts_source_path_for_js_specifier(candidate: &Path) -> Option<PathBuf> {
    let extension = candidate.extension()?.to_str()?;
    let source_extensions: &[&str] = match extension {
        "js" => &["ts", "tsx", "d.ts"],
        "jsx" => &["tsx", "ts", "d.ts"],
        "mjs" => &["mts", "d.mts", "ts", "d.ts"],
        "cjs" => &["cts", "d.cts", "ts", "d.ts"],
        _ => return None,
    };

    for source_extension in source_extensions {
        let source_candidate = candidate.with_extension(source_extension);
        if source_candidate.is_file() {
            return Some(source_candidate.canonicalize().unwrap_or(source_candidate));
        }
    }
    None
}

pub(super) fn parse_ts<R>(
    source: &str,
    path: impl AsRef<Path>,
    f: impl for<'a> FnOnce(&'a oxc_ast::ast::Program<'a>) -> R,
) -> Option<R> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path.as_ref()).unwrap_or_default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    if ret.panicked {
        None
    } else {
        Some(f(&ret.program))
    }
}
