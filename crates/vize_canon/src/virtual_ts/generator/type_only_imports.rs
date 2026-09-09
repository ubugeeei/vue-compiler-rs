//! Syntactic `import type` binding extraction for component-value shadowing.

use oxc_allocator::Allocator;
use oxc_ast::ast::{ImportDeclarationSpecifier, ImportOrExportKind, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;
use vize_carton::{CompactString, FxHashSet};
use vize_croquis::Croquis;

use super::global_components::GlobalComponentPlan;

pub(super) fn should_collect_syntactic_type_only_imported_names(
    summary: &Croquis,
    global_components: &GlobalComponentPlan<'_>,
) -> bool {
    (global_components.enabled() && !summary.component_usages.is_empty())
        || !summary.used_components.is_empty()
        || summary.macros.define_props().is_some()
        || !summary.macros.models().is_empty()
}

pub(super) fn collect_syntactic_type_only_imported_names(
    summary: &Croquis,
    script_content: Option<&str>,
) -> FxHashSet<CompactString> {
    let Some(script) = script_content else {
        return FxHashSet::default();
    };

    summary
        .import_statements
        .iter()
        .flat_map(|imp| {
            let text = script
                .get(imp.start as usize..imp.end as usize)
                .unwrap_or("");
            extract_syntactic_type_only_import_names(text)
                .into_iter()
                .map(CompactString::new)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn extract_syntactic_type_only_import_names(import_text: &str) -> Vec<&str> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, import_text, SourceType::ts()).parse();
    let Some(Statement::ImportDeclaration(declaration)) = parsed.program.body.first() else {
        return Vec::new();
    };

    declaration
        .specifiers
        .iter()
        .flatten()
        .filter_map(|specifier| {
            let span = match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(specifier)
                    if declaration.import_kind == ImportOrExportKind::Type
                        || specifier.import_kind == ImportOrExportKind::Type =>
                {
                    specifier.local.span
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier)
                    if declaration.import_kind == ImportOrExportKind::Type =>
                {
                    specifier.local.span
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier)
                    if declaration.import_kind == ImportOrExportKind::Type =>
                {
                    specifier.local.span
                }
                _ => return None,
            };
            import_text.get(span.start as usize..span.end as usize)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::extract_syntactic_type_only_import_names;

    #[test]
    fn syntactic_type_only_import_names_keep_type_query_imports_out_of_value_space() {
        assert_eq!(
            extract_syntactic_type_only_import_names(
                "import type DefaultBadge, { Badge, type Props as BadgeProps } from 'pkg'"
            ),
            ["DefaultBadge", "Badge", "BadgeProps"]
        );
        assert_eq!(
            extract_syntactic_type_only_import_names(
                "import { value, type Props as LocalProps } from 'pkg'"
            ),
            ["LocalProps"]
        );
        assert!(extract_syntactic_type_only_import_names("import { value } from 'pkg'").is_empty());
    }
}
