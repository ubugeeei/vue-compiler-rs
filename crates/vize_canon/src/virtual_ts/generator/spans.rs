//! Span collection/merging, template-referenced-name discovery, and
//! `export default` rewriting used while assembling the virtual TypeScript.

use vize_atelier_sfc::script::resolve_template_read_identifiers;
use vize_carton::{FxHashSet, String};
use vize_croquis::{Croquis, ScopeData};

pub(super) fn template_usage(
    summary: &Croquis,
    template_ast: Option<&vize_relief::RootNode<'_>>,
    generation_options: crate::virtual_ts::types::VirtualTsGenerationOptions<'_>,
) -> (FxHashSet<String>, bool) {
    let names = collect_template_referenced_names(
        summary,
        template_ast,
        generation_options.extra_template_referenced_names,
    );
    let has_scope = template_ast.is_some() || !names.is_empty();
    (names, has_scope)
}

fn collect_template_referenced_names(
    summary: &Croquis,
    template_ast: Option<&vize_relief::RootNode<'_>>,
    extra_template_referenced_names: Option<&FxHashSet<String>>,
) -> FxHashSet<String> {
    let mut names = FxHashSet::default();

    if let Some(template_ast) = template_ast {
        names.extend(resolve_template_read_identifiers(template_ast));
    }

    if let Some(extra_names) = extra_template_referenced_names {
        names.extend(extra_names.iter().cloned());
    }

    for expression in &summary.template_expressions {
        collect_expression_identifiers(&mut names, expression.content.as_str());
        if let Some(guard) = expression.vif_guard.as_ref() {
            collect_expression_identifiers(&mut names, guard.as_str());
        }
    }

    for usage in &summary.component_usages {
        names.insert(usage.name.as_str().into());
        if let Some(guard) = usage.vif_guard.as_ref() {
            collect_expression_identifiers(&mut names, guard.as_str());
        }
        for prop in &usage.props {
            if prop.is_dynamic
                && let Some(value) = prop.value.as_ref()
            {
                collect_expression_identifiers(&mut names, value.as_str());
            }
        }
        for event in &usage.events {
            if let Some(handler) = event.handler.as_ref() {
                collect_expression_identifiers(&mut names, handler.as_str());
            }
        }
    }

    for component in &summary.used_components {
        names.insert(component.as_str().into());
    }

    for scope in summary.scopes.iter() {
        match scope.data() {
            ScopeData::VFor(data) => {
                collect_expression_identifiers(&mut names, data.source.as_str());
                if let Some(key_expression) = data.key_expression.as_ref() {
                    collect_expression_identifiers(&mut names, key_expression.as_str());
                }
            }
            ScopeData::EventHandler(data) => {
                if let Some(handler) = data.handler_expression.as_ref() {
                    collect_expression_identifiers(&mut names, handler.as_str());
                }
            }
            _ => {}
        }
    }

    names
}

fn collect_expression_identifiers(names: &mut FxHashSet<String>, expression: &str) {
    for identifier in vize_croquis::drawer::extract_identifiers_oxc(expression) {
        names.insert(identifier.as_str().into());
    }
}

pub(super) fn merge_overlapping_spans(mut spans: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    spans.retain(|(start, end)| start < end);
    spans.sort_by_key(|&(start, end)| (start, end));

    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(spans.len());
    for (start, end) in spans {
        if let Some((_, previous_end)) = merged.last_mut()
            && start <= *previous_end
        {
            *previous_end = (*previous_end).max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

/// Generated identifier resolving Vue's `defineComponent` (see
/// `DEFINE_COMPONENT_HELPER`). Plain object-literal default exports are
/// wrapped in a call to it so TypeScript binds `this` inside
/// computed/methods via Vue's `ThisType` machinery instead of the bare
/// object literal (which produced TS2339 false positives).
pub(super) const DEFINE_COMPONENT_REF: &str = "__vizeDefineComponent";

/// Module-scope declaration for `DEFINE_COMPONENT_REF`. Uses the same
/// `import('vue')` reference form as the other generated vue helpers so it
/// never collides with user imports.
pub(super) const DEFINE_COMPONENT_HELPER: &str =
    "declare const __vizeDefineComponent: typeof import('vue').defineComponent;\n";

pub(super) fn rewrite_export_default_for_module_scope(
    text: &str,
    default_object: Option<(usize, usize, usize)>,
    default_expr: Option<(usize, usize, usize)>,
) -> String {
    // Plain `export default { ... }` (the Options API shape) is wrapped with
    // Vue's `defineComponent`; every other default-export shape is rewritten to
    // a bare `const __default__ = <expr>` using the AST-provided byte offsets.
    if let Some(output) = wrap_default_export_object(text, default_object) {
        return output;
    }
    if let Some(output) = rewrite_default_export_expression(text, default_expr) {
        return output;
    }

    // No rewriteable default export in this span (e.g. a re-export statement
    // such as `export { default } from './x'`): leave the text untouched.
    text.into()
}

/// Rewrite an arbitrary `export default <expr>` to `const __default__ = <expr>`
/// using the `(export_start, expr_start, expr_end)` offsets (relative to
/// `text`) located by the AST. The `export default` keyword run
/// (`export_start..expr_start`) is dropped and the exported expression
/// (`expr_start..expr_end`) is copied verbatim, so source formatting —
/// `export default{` with no space, multi-line calls, decorated/anonymous
/// classes — is preserved exactly. Returns `None` when the offsets do not
/// describe a default export inside `text`.
fn rewrite_default_export_expression(
    text: &str,
    default_expr: Option<(usize, usize, usize)>,
) -> Option<String> {
    const EXPORT_DEFAULT: &str = "export default";
    const REPLACEMENT: &str = "const __default__ =";

    let (export_start, expr_start, expr_end) = default_expr?;
    if expr_end > text.len() || expr_start >= expr_end || export_start >= expr_start {
        return None;
    }
    let keyword_end = export_start.checked_add(EXPORT_DEFAULT.len())?;
    if keyword_end > expr_start
        || !text.is_char_boundary(export_start)
        || !text.is_char_boundary(expr_start)
        || !text.is_char_boundary(expr_end)
        || !text[export_start..].starts_with(EXPORT_DEFAULT)
    {
        return None;
    }

    let mut output = String::with_capacity(text.len() + REPLACEMENT.len());
    output.push_str(&text[..export_start]);
    output.push_str(REPLACEMENT);
    // `keyword_end..expr_start` is the inter-token whitespace/comment run
    // between the keyword and the expression (empty for `export default{`).
    output.push_str(&text[keyword_end..expr_start]);
    output.push_str(&text[expr_start..expr_end]);
    output.push_str(&text[expr_end..]);
    Some(output)
}

/// Rewrite `export default { ... }` to
/// `const __default__ = __vizeDefineComponent({ ... })` using the
/// `(export_start, object_start, object_end)` offsets (relative to `text`)
/// located by the AST. Returns `None` when the offsets do not describe a
/// plain object-literal default export inside `text`.
fn wrap_default_export_object(
    text: &str,
    default_object: Option<(usize, usize, usize)>,
) -> Option<String> {
    const EXPORT_DEFAULT: &str = "export default";

    let (export_start, object_start, object_end) = default_object?;
    if object_end > text.len() || object_start >= object_end {
        return None;
    }
    let keyword_end = export_start.checked_add(EXPORT_DEFAULT.len())?;
    if keyword_end > object_start
        || !text.is_char_boundary(export_start)
        || !text.is_char_boundary(object_start)
        || !text.is_char_boundary(object_end)
        || !text[export_start..].starts_with(EXPORT_DEFAULT)
        || !text[object_start..].starts_with('{')
    {
        return None;
    }

    let mut output =
        String::with_capacity(text.len() + DEFINE_COMPONENT_REF.len() + EXPORT_DEFAULT.len());
    output.push_str(&text[..export_start]);
    output.push_str("const __default__ =");
    output.push_str(&text[keyword_end..object_start]);
    output.push_str(DEFINE_COMPONENT_REF);
    output.push('(');
    output.push_str(&text[object_start..object_end]);
    output.push(')');
    output.push_str(&text[object_end..]);
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::rewrite_export_default_for_module_scope;
    use crate::virtual_ts::generator::options_api::find_default_export_targets;

    /// Drive the rewrite exactly as the generator does: classify the default
    /// export with the shared single-parse `find_default_export_targets`, then
    /// feed the resulting AST byte offsets (here the whole script is one span,
    /// so offsets need no re-basing) into the rewrite.
    fn rewrite(script: &str) -> super::String {
        let targets = find_default_export_targets(script);
        rewrite_export_default_for_module_scope(script, targets.object, targets.expr)
    }

    #[test]
    fn plain_object_literal_is_wrapped_with_define_component() {
        // The Options API object-literal shape is unchanged by this PR: it is
        // still wrapped in `__vizeDefineComponent(...)` via the object path.
        let out = rewrite("export default { name: 'A' }");
        assert_eq!(
            out.as_str(),
            "const __default__ = __vizeDefineComponent({ name: 'A' })"
        );
    }

    #[test]
    fn define_component_call_is_rewritten_via_span() {
        // `export default defineComponent({...})` used to fall to the line
        // scanner; it now slices on the call expression span and is NOT wrapped
        // again in `defineComponent`.
        let out = rewrite("export default defineComponent({ name: 'A' })");
        assert_eq!(
            out.as_str(),
            "const __default__ = defineComponent({ name: 'A' })"
        );
    }

    #[test]
    fn anonymous_class_is_rewritten_via_span() {
        // Anonymous default classes have no name to alias by, so the class
        // path declines them; the generic expr span handles them instead.
        let out = rewrite("export default class { x = 1 }");
        assert_eq!(out.as_str(), "const __default__ = class { x = 1 }");
    }

    #[test]
    fn named_class_is_left_to_the_per_line_class_path() {
        // A *named* class default export is captured as `class`, not `expr`,
        // and the module-scope rewrite leaves it untouched (the per-line
        // setup-scope path keeps decorators on a real class declaration —
        // PR #1434). The module-scope function is a no-op here.
        let script = "export default class Foo { x = 1 }";
        let out = rewrite(script);
        assert_eq!(out.as_str(), script);
    }

    #[test]
    fn export_default_with_no_space_is_rewritten_correctly() {
        // `export default{` (no space before the brace). The old line scanner
        // required `export default` be followed by whitespace, so it failed to
        // match and emitted `export default{ ... }` verbatim — invalid at
        // module scope. The span path drops the keyword run exactly.
        let out = rewrite("export default{ name: 'A' }");
        assert_eq!(
            out.as_str(),
            "const __default__ =__vizeDefineComponent({ name: 'A' })"
        );
    }

    #[test]
    fn define_component_call_with_no_space_is_rewritten_correctly() {
        // Same no-space hazard, but a call expression so it takes the generic
        // expr path rather than the object-wrap path.
        let out = rewrite("export default(defineComponent({ name: 'A' }))");
        assert_eq!(
            out.as_str(),
            "const __default__ =(defineComponent({ name: 'A' }))"
        );
    }

    #[test]
    fn multi_line_call_is_rewritten_across_lines() {
        // An awkwardly formatted multi-line default export. The old scanner
        // rewrote only the first physical line containing `export default`,
        // leaving trailing lines under a dangling `const`; the span path
        // replaces just the keyword run and keeps the expression — including
        // its newlines — verbatim.
        let script = "export default\n  defineComponent({\n    name: 'A',\n  })";
        let out = rewrite(script);
        assert_eq!(
            out.as_str(),
            "const __default__ =\n  defineComponent({\n    name: 'A',\n  })"
        );
    }

    #[test]
    fn leading_code_before_export_default_is_preserved() {
        // Statements before the default export are copied verbatim; only the
        // keyword run is replaced.
        let script = "const a = 1;\nexport default defineComponent({ a })";
        let out = rewrite(script);
        assert_eq!(
            out.as_str(),
            "const a = 1;\nconst __default__ = defineComponent({ a })"
        );
    }

    #[test]
    fn re_export_default_is_left_untouched() {
        // `export { default } from './x'` is not a default-export declaration;
        // neither offset is produced and the text is returned unchanged.
        let script = "export { default } from './x'";
        let out = rewrite(script);
        assert_eq!(out.as_str(), script);
    }
}
