//! Import processing utilities.
//!
//! This module handles processing import statements, including
//! removing TypeScript type-only imports and extracting identifiers.

use oxc_allocator::Allocator;
use oxc_ast::ast::{ImportDeclarationSpecifier, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Process import statement to remove TypeScript type-only imports using OXC
/// Returns None if the entire import should be removed, Some(processed) otherwise
pub fn process_import_for_types(import: &str) -> Option<String> {
    let import = import.trim();

    // Parse the import statement with OXC
    let allocator = Allocator::default();
    let source_type = SourceType::ts();
    let parser = Parser::new(&allocator, import, source_type);
    let result = parser.parse();

    if result.errors.is_empty() {
        for stmt in &result.program.body {
            if let Statement::ImportDeclaration(decl) = stmt {
                // Skip type-only imports: import type { ... } from '...'
                if decl.import_kind.is_type() {
                    return None;
                }

                // Check if there are any specifiers
                if let Some(specifiers) = &decl.specifiers {
                    // Filter out type-only specifiers
                    let value_specifiers: Vec<&ImportDeclarationSpecifier> = specifiers
                        .iter()
                        .filter(|spec| match spec {
                            ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                !s.import_kind.is_type()
                            }
                            _ => true,
                        })
                        .collect();

                    if value_specifiers.is_empty() {
                        // All specifiers were type imports
                        return None;
                    }

                    if value_specifiers.len() != specifiers.len() {
                        // Some specifiers were filtered out, rebuild the import
                        let source = decl.source.value.as_str();
                        let specifier_strs: Vec<String> = value_specifiers
                            .iter()
                            .map(|spec| match spec {
                                ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                    let imported = s.imported.name().as_str();
                                    let local = s.local.name.as_str();
                                    if imported == local {
                                        imported.to_string()
                                    } else {
                                        format!("{} as {}", imported, local)
                                    }
                                }
                                ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                                    s.local.name.to_string()
                                }
                                ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                                    format!("* as {}", s.local.name)
                                }
                            })
                            .collect();

                        let new_import = format!(
                            "import {{ {} }} from '{}'\n",
                            specifier_strs.join(", "),
                            source
                        );
                        return Some(new_import);
                    }
                }
            }
        }
    }

    // Regular import or parse failed, return as-is
    Some(import.to_string() + "\n")
}

/// Extract all identifiers from an import statement (including default imports)
pub fn extract_import_identifiers(import: &str) -> Vec<String> {
    let import = import.trim();
    let mut identifiers = Vec::new();

    // Parse the import statement with OXC
    let allocator = Allocator::default();
    let source_type = SourceType::ts();
    let parser = Parser::new(&allocator, import, source_type);
    let result = parser.parse();

    if result.errors.is_empty() {
        for stmt in &result.program.body {
            if let Statement::ImportDeclaration(decl) = stmt {
                // Skip type-only imports
                if decl.import_kind.is_type() {
                    continue;
                }

                if let Some(specifiers) = &decl.specifiers {
                    for spec in specifiers {
                        match spec {
                            ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                // Skip type-only specifiers
                                if !s.import_kind.is_type() {
                                    identifiers.push(s.local.name.to_string());
                                }
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                                identifiers.push(s.local.name.to_string());
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                                identifiers.push(s.local.name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    identifiers
}
