//! Template expression statement generation.
//!
//! Emits TypeScript `void (...)` statements for template expressions, with
//! optional v-if narrowing, and delegates recognized v-if chains to the
//! control-flow emitter in [`super::vif_chain`].

use super::super::helpers::generated_text_range;
use super::super::types::VizeMapping;
use super::component_ref_callbacks::generate_component_ref_callback_statement;
use super::directive_values::generate_directive_value_statement;
use super::native_props::generate_native_prop_statement;
use super::reserved_props::rewrite_reserved_template_prop;
use super::value_checks::TemplateValueChecks;
use super::vif_chain::{VifControlFlowChain, emit_vif_control_flow_chain};
use crate::virtual_ts::scope::{append_ignored_vif_guard_open, remove_enclosing_vif_guard_prefix};
use std::borrow::Cow;
use vize_carton::CompactString;
use vize_carton::FxHashSet;
use vize_carton::String;
use vize_carton::append;
use vize_carton::cstr;
use vize_carton::profile;
use vize_croquis::croquis::{TemplateExpression, TemplateExpressionKind};
use vize_croquis::drawer::strip_js_comments;

/// Generate template expressions, compacting recognized v-if chains into
/// TypeScript control-flow blocks.
pub(crate) fn generate_expressions(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    exprs: &[&TemplateExpression],
    template_prop_names: &FxHashSet<String>,
    context: &ExpressionListEmitContext<'_>,
) {
    let mut index = 0;
    while index < exprs.len() {
        if context
            .skipped_expression_ranges
            .contains(&(exprs[index].start, exprs[index].end))
        {
            index += 1;
            continue;
        }
        if let Some(chain) = VifControlFlowChain::collect(exprs, index) {
            emit_vif_control_flow_chain(ts, mappings, exprs, &chain, template_prop_names, context);
            index = chain.end;
            continue;
        }

        profile!(
            "canon.virtual_ts.generate_expression",
            generate_expression(
                ts,
                mappings,
                exprs[index],
                template_prop_names,
                context.template_offset,
                context.indent,
                context.checks,
            )
        );
        index += 1;
    }
}

/// Generate expressions inside control flow that already enforces a common
/// v-if guard, removing only that exact top-level guard prefix first.
pub(crate) fn generate_expressions_in_enclosing_guard(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    exprs: &[&TemplateExpression],
    template_prop_names: &FxHashSet<String>,
    context: &ExpressionListEmitContext<'_>,
    enclosing_guard: Option<&str>,
) {
    let Some(enclosing_guard) = enclosing_guard else {
        generate_expressions(ts, mappings, exprs, template_prop_names, context);
        return;
    };

    let adjusted_expressions: Vec<_> = exprs
        .iter()
        .map(|expr| {
            let mut adjusted = (**expr).clone();
            adjusted.vif_guard = adjusted.vif_guard.as_ref().and_then(|guard| {
                remove_enclosing_vif_guard_prefix(guard.as_str(), enclosing_guard)
                    .map(|guard| CompactString::new(guard.as_str()))
            });
            adjusted
        })
        .collect();
    let adjusted_expression_refs: Vec<_> = adjusted_expressions.iter().collect();
    generate_expressions(
        ts,
        mappings,
        &adjusted_expression_refs,
        template_prop_names,
        context,
    );
}

pub(crate) struct ExpressionListEmitContext<'a> {
    pub(crate) skipped_expression_ranges: &'a FxHashSet<(u32, u32)>,
    pub(crate) template_offset: u32,
    pub(crate) indent: &'a str,
    pub(crate) checks: TemplateValueChecks<'a>,
}

impl<'a> ExpressionListEmitContext<'a> {
    pub(crate) fn new(
        skipped_expression_ranges: &'a FxHashSet<(u32, u32)>,
        template_offset: u32,
        indent: &'a str,
        checks: TemplateValueChecks<'a>,
    ) -> Self {
        Self {
            skipped_expression_ranges,
            template_offset,
            indent,
            checks,
        }
    }
}

/// Generate a template expression, wrapping guarded expressions so TypeScript
/// can narrow them.
pub(crate) fn generate_expression(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    expr: &vize_croquis::TemplateExpression,
    template_prop_names: &FxHashSet<String>,
    template_offset: u32,
    indent: &str,
    checks: TemplateValueChecks<'_>,
) {
    if let Some(ref guard) = expr.vif_guard {
        if expr.kind == TemplateExpressionKind::VIf {
            generate_vif_guard_expression(
                ts,
                mappings,
                expr,
                guard.as_str(),
                template_prop_names,
                template_offset,
                indent,
            );
            return;
        }

        let trimmed_guard = guard.as_str().trim();
        let rewritten_guard = rewrite_reserved_template_prop(trimmed_guard, template_prop_names);
        let generated_guard = rewritten_guard
            .as_ref()
            .map_or_else(|| guard.as_str(), |s| s.as_str());
        // Wrap in if block for type narrowing
        let gen_guard_start = ts.len();
        append_ignored_vif_guard_open(ts, indent, generated_guard, "Narrowing-only guard");
        let gen_guard_end = ts.len();
        mappings.push(VizeMapping {
            gen_range: generated_text_range(
                &ts[gen_guard_start..gen_guard_end],
                generated_guard,
                gen_guard_start,
            ),
            src_range: (template_offset + expr.start) as usize
                ..(template_offset + expr.end) as usize,
            sub_spans: Vec::new(),
        });
        generate_expression_statement(
            ts,
            mappings,
            expr,
            template_prop_names,
            template_offset,
            &cstr!("{indent}  "),
            checks,
        );
        append!(*ts, "{indent}}}\n");
    } else {
        generate_expression_statement(
            ts,
            mappings,
            expr,
            template_prop_names,
            template_offset,
            indent,
            checks,
        );
    }
}

fn generate_vif_guard_expression(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    expr: &TemplateExpression,
    guard: &str,
    template_prop_names: &FxHashSet<String>,
    template_offset: u32,
    indent: &str,
) {
    let src_start = (template_offset + expr.start) as usize;
    let src_end = (template_offset + expr.end) as usize;
    let expression = profile!(
        "canon.virtual_ts.expression.strip_comments",
        strip_js_comments(expr.content.as_str())
    );
    let trimmed_expression = expression.as_ref().trim();
    let rewritten_expression =
        rewrite_reserved_template_prop(trimmed_expression, template_prop_names);
    let generated_expression = rewritten_expression
        .as_ref()
        .map_or_else(|| expression.as_ref(), |s| s.as_str());
    let trimmed_guard = guard.trim();
    let rewritten_guard = rewrite_reserved_template_prop(trimmed_guard, template_prop_names);
    let generated_guard = rewritten_guard
        .as_ref()
        .map_or_else(|| guard, |s| s.as_str());
    let mapping_needle = if generated_guard.contains(generated_expression) {
        generated_expression
    } else {
        generated_guard
    };

    let gen_stmt_start = ts.len();
    append!(*ts, "{indent}if ({generated_guard}) {{\n");
    let gen_stmt_end = ts.len();
    mappings.push(VizeMapping {
        gen_range: generated_text_range(
            &ts[gen_stmt_start..gen_stmt_end],
            mapping_needle,
            gen_stmt_start,
        ),
        src_range: src_start..src_end,
        sub_spans: Vec::new(),
    });
    append!(
        *ts,
        "{indent}  // @vize-map: expr -> {src_start}:{src_end}\n",
    );
    append!(*ts, "{indent}}}\n");
}

pub(super) fn generate_expression_statement(
    ts: &mut String,
    mappings: &mut Vec<VizeMapping>,
    expr: &TemplateExpression,
    template_prop_names: &FxHashSet<String>,
    template_offset: u32,
    indent: &str,
    checks: TemplateValueChecks<'_>,
) {
    let src_start = (template_offset + expr.start) as usize;
    let src_end = (template_offset + expr.end) as usize;
    let is_component_ref_callback = checks
        .component_ref_callbacks
        .contains_key(&(expr.start, expr.end));
    let expression = if is_component_ref_callback {
        Cow::Borrowed(expr.content.as_str())
    } else {
        profile!(
            "canon.virtual_ts.expression.strip_comments",
            strip_js_comments(expr.content.as_str())
        )
    };
    let trimmed_expression = expression.as_ref().trim();
    if trimmed_expression.is_empty() {
        return;
    }
    let statement_expression =
        if expr.kind == TemplateExpressionKind::VOn && trimmed_expression.starts_with(';') {
            trimmed_expression
                .trim_start_matches(|character: char| character == ';' || character.is_whitespace())
        } else {
            expression.as_ref()
        };
    if statement_expression.is_empty() {
        return;
    }
    let rewritten_expression =
        rewrite_reserved_template_prop(statement_expression.trim(), template_prop_names);
    let generated_expression = rewritten_expression
        .as_ref()
        .map_or_else(|| statement_expression, |s| s.as_str());
    let mapping_needle = if rewritten_expression.is_some() {
        generated_expression
    } else {
        statement_expression
    };

    if let Some(native_prop) = checks.native_props.get(&(expr.start, expr.end)) {
        generate_native_prop_statement(
            ts,
            mappings,
            expr,
            native_prop,
            generated_expression,
            template_offset,
            indent,
        );
        return;
    }

    if is_component_ref_callback {
        generate_component_ref_callback_statement(
            ts,
            mappings,
            expr,
            generated_expression,
            template_offset,
            indent,
        );
        return;
    }

    if expr.kind == TemplateExpressionKind::CustomDirective
        && let Some(directive_value) = checks.directive_values.get(&(expr.start, expr.end))
    {
        generate_directive_value_statement(
            ts,
            mappings,
            expr,
            directive_value,
            generated_expression,
            template_offset,
            indent,
        );
        return;
    }

    let gen_stmt_start = ts.len();
    append!(
        *ts,
        "{indent}void ({}); // {}\n",
        generated_expression,
        expr.kind.as_str()
    );
    let gen_stmt_end = ts.len();
    mappings.push(VizeMapping {
        gen_range: generated_text_range(
            &ts[gen_stmt_start..gen_stmt_end],
            mapping_needle,
            gen_stmt_start,
        ),
        src_range: src_start..src_end,
        sub_spans: Vec::new(),
    });
    append!(*ts, "{indent}// @vize-map: expr -> {src_start}:{src_end}\n",);
}
