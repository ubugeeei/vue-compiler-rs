//! TypeScript transformation utilities.
//!
//! This module handles transforming TypeScript code to JavaScript using OXC.

use oxc_allocator::Allocator;
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{TransformOptions, Transformer, TypeScriptOptions};

/// Transform TypeScript code to JavaScript using OXC
pub fn transform_typescript_to_js(code: &str) -> String {
    let allocator = Allocator::default();
    let source_type = SourceType::ts();
    let parser = Parser::new(&allocator, code, source_type);
    let parse_result = parser.parse();

    if !parse_result.errors.is_empty() {
        // If parsing fails, return original code
        return code.to_string();
    }

    let mut program = parse_result.program;

    // Run semantic analysis to get symbols and scopes
    let semantic_ret = SemanticBuilder::new()
        .with_excess_capacity(2.0)
        .build(&program);

    if !semantic_ret.errors.is_empty() {
        // If semantic analysis fails, return original code
        return code.to_string();
    }

    let (symbols, scopes) = semantic_ret.semantic.into_symbol_table_and_scope_tree();

    // Transform TypeScript to JavaScript
    // Use only_remove_type_imports to preserve component imports that appear "unused"
    // but are actually used by the template
    let transform_options = TransformOptions {
        typescript: TypeScriptOptions {
            only_remove_type_imports: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let ret = Transformer::new(&allocator, std::path::Path::new(""), &transform_options)
        .build_with_symbols_and_scopes(symbols, scopes, &mut program);

    if !ret.errors.is_empty() {
        // If transformation fails, return original code
        return code.to_string();
    }

    // Generate JavaScript code
    // Replace tabs with 2 spaces for consistent indentation
    Codegen::new().build(&program).code.replace('\t', "  ")
}
