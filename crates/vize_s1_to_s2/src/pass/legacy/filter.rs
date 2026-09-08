//! `vue.filter` → `_filter_*(...)` call text, re-admitted as JS.

use alloc::vec::Vec as StdVec;

use vize_davinci::side_table::SideTable;
use vize_s0::{Allocator, String};
use vize_s2::expr::{ExprRef, VueFilterApp, VueFilterExpr};
use vize_s2::op::{BindingOp, DynamicName, Op};

use crate::emit::js::asset_ident;
use crate::lower::TextParts;
use crate::pass::walk::PageWalk;

use super::LegacyFacts;

mod compound;

/// Rewrite every [`ExprRef::Filter`] in the tree into the Vue 2 wrap
/// (`a | f` → `_filter_f(a)`, `a | f(b)` → `_filter_f(a,b)`). Mixed text
/// runs carry their dynamic parts in the text side table, so the pass
/// rewrites those part expressions, refreshes the compound spelling, and
/// rechecks the lowering-published fact law.
pub(super) fn rewrite<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    ops: &mut [Op<'a>],
    texts: &mut SideTable<TextParts>,
    facts: &mut LegacyFacts,
) {
    let mut walk = PageWalk::new();
    rewrite_ops(allocator, source, &mut walk, ops, texts, facts);
}

fn rewrite_ops<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    walk: &mut PageWalk,
    ops: &mut [Op<'a>],
    texts: &mut SideTable<TextParts>,
    facts: &mut LegacyFacts,
) {
    for op in ops.iter_mut() {
        let id = walk.mint();
        match op {
            Op::Element(element) => {
                rewrite_bindings(allocator, &mut element.bindings, &mut facts.filters);
                walk.skip(element.bindings.len());
                rewrite_ops(
                    allocator,
                    source,
                    walk,
                    &mut element.children.ops,
                    texts,
                    facts,
                );
            }
            Op::Component(component) => {
                if bindings_contain_filter(&component.bindings) {
                    facts.filter_helper_precedes_components = true;
                }
                rewrite_bindings(allocator, &mut component.bindings, &mut facts.filters);
                walk.skip(component.bindings.len());
                rewrite_ops(
                    allocator,
                    source,
                    walk,
                    &mut component.children.ops,
                    texts,
                    facts,
                );
            }
            Op::Slot(slot) => {
                rewrite_name(allocator, &mut slot.name, &mut facts.filters);
                rewrite_bindings(allocator, &mut slot.bindings, &mut facts.filters);
                walk.skip(slot.bindings.len());
                rewrite_ops(
                    allocator,
                    source,
                    walk,
                    &mut slot.fallback.ops,
                    texts,
                    facts,
                );
            }
            Op::Interpolation(interp) => {
                rewrite_expr(allocator, &mut interp.expression, &mut facts.filters);
                compound::rewrite(allocator, source, id, interp, texts, &mut facts.filters);
            }
            Op::If(if_op) => {
                for branch in if_op.branches.iter_mut() {
                    if let Some(condition) = &mut branch.condition {
                        rewrite_expr(allocator, condition, &mut facts.filters);
                    }
                    rewrite_ops(
                        allocator,
                        source,
                        walk,
                        &mut branch.region.ops,
                        texts,
                        facts,
                    );
                }
            }
            Op::For(for_op) => {
                rewrite_expr(allocator, &mut for_op.binding.source, &mut facts.filters);
                rewrite_expr(allocator, &mut for_op.binding.value, &mut facts.filters);
                if let Some(key) = &mut for_op.binding.key {
                    rewrite_expr(allocator, key, &mut facts.filters);
                }
                if let Some(index) = &mut for_op.binding.index {
                    rewrite_expr(allocator, index, &mut facts.filters);
                }
                rewrite_ops(
                    allocator,
                    source,
                    walk,
                    &mut for_op.region.ops,
                    texts,
                    facts,
                );
            }
            Op::Text(_) | Op::Comment(_) => {}
        }
    }
}

fn bindings_contain_filter(bindings: &[BindingOp<'_>]) -> bool {
    bindings.iter().any(binding_contains_filter)
}

fn binding_contains_filter(binding: &BindingOp<'_>) -> bool {
    match binding {
        BindingOp::Bind(bind) => {
            bind.name.as_ref().is_some_and(name_contains_filter)
                || bind.value.as_ref().is_some_and(expr_contains_filter)
        }
        BindingOp::On(on) => {
            on.name.as_ref().is_some_and(name_contains_filter)
                || on.handler.as_ref().is_some_and(expr_contains_filter)
        }
        BindingOp::Model(model) => {
            expr_contains_filter(&model.contract.read)
                || expr_contains_filter(&model.contract.write)
        }
        BindingOp::SlotContent(content) => {
            content.name.as_ref().is_some_and(name_contains_filter)
                || content.params.as_ref().is_some_and(expr_contains_filter)
        }
        BindingOp::VueDirective(directive) => {
            directive
                .argument
                .as_ref()
                .is_some_and(name_contains_filter)
                || directive.value.as_ref().is_some_and(expr_contains_filter)
        }
        BindingOp::VueCssBind(bind) => expr_contains_filter(&bind.value),
        BindingOp::VueSync(sync) => expr_contains_filter(&sync.value),
        BindingOp::VueSlotScope(scope) => scope.params.as_ref().is_some_and(expr_contains_filter),
        BindingOp::VueOnce(_) => false,
        BindingOp::VueMemo(memo) => expr_contains_filter(&memo.value),
        BindingOp::VueShow(show) => expr_contains_filter(&show.value),
        BindingOp::VueHtml(html) => html.value.as_ref().is_some_and(expr_contains_filter),
        BindingOp::VueText(text) => text.value.as_ref().is_some_and(expr_contains_filter),
        BindingOp::VueCloak(_) => false,
    }
}

fn name_contains_filter(name: &DynamicName<'_>) -> bool {
    matches!(name, DynamicName::Dynamic(expr) if expr_contains_filter(expr))
}

fn expr_contains_filter(expr: &ExprRef<'_>) -> bool {
    matches!(expr, ExprRef::Filter(_))
}

fn rewrite_bindings<'a>(
    allocator: &'a Allocator,
    bindings: &mut [BindingOp<'a>],
    filters: &mut StdVec<String>,
) {
    for binding in bindings.iter_mut() {
        match binding {
            BindingOp::Bind(bind) => {
                if let Some(name) = &mut bind.name {
                    rewrite_name(allocator, name, filters);
                }
                if let Some(value) = &mut bind.value {
                    rewrite_expr(allocator, value, filters);
                }
            }
            BindingOp::On(on) => {
                if let Some(name) = &mut on.name {
                    rewrite_name(allocator, name, filters);
                }
                if let Some(handler) = &mut on.handler {
                    rewrite_expr(allocator, handler, filters);
                }
            }
            BindingOp::Model(model) => {
                rewrite_expr(allocator, &mut model.contract.read, filters);
                rewrite_expr(allocator, &mut model.contract.write, filters);
            }
            BindingOp::SlotContent(content) => {
                if let Some(name) = &mut content.name {
                    rewrite_name(allocator, name, filters);
                }
                if let Some(params) = &mut content.params {
                    rewrite_expr(allocator, params, filters);
                }
            }
            BindingOp::VueDirective(directive) => {
                if let Some(argument) = &mut directive.argument {
                    rewrite_name(allocator, argument, filters);
                }
                if let Some(value) = &mut directive.value {
                    rewrite_expr(allocator, value, filters);
                }
            }
            BindingOp::VueCssBind(bind) => rewrite_expr(allocator, &mut bind.value, filters),
            BindingOp::VueSync(sync) => rewrite_expr(allocator, &mut sync.value, filters),
            BindingOp::VueSlotScope(scope) => {
                if let Some(params) = &mut scope.params {
                    rewrite_expr(allocator, params, filters);
                }
            }
            BindingOp::VueOnce(_) => {}
            BindingOp::VueMemo(memo) => rewrite_expr(allocator, &mut memo.value, filters),
            BindingOp::VueShow(show) => rewrite_expr(allocator, &mut show.value, filters),
            BindingOp::VueHtml(html) => {
                if let Some(value) = &mut html.value {
                    rewrite_expr(allocator, value, filters);
                }
            }
            BindingOp::VueText(text) => {
                if let Some(value) = &mut text.value {
                    rewrite_expr(allocator, value, filters);
                }
            }
            BindingOp::VueCloak(_) => {}
        }
    }
}

fn rewrite_name<'a>(
    allocator: &'a Allocator,
    name: &mut DynamicName<'a>,
    filters: &mut StdVec<String>,
) {
    if let DynamicName::Dynamic(expr) = name {
        rewrite_expr(allocator, expr, filters);
    }
}

fn rewrite_expr<'a>(
    allocator: &'a Allocator,
    expr: &mut ExprRef<'a>,
    filters: &mut StdVec<String>,
) {
    let ExprRef::Filter(filter) = *expr else {
        return;
    };
    record_filters(filter, filters);
    *expr = wrap(allocator, filter);
}

fn record_filters(filter: &VueFilterExpr<'_>, filters: &mut StdVec<String>) {
    for app in &filter.filters {
        if !filters.iter().any(|seen| seen.as_str() == app.name) {
            filters.push(String::from(app.name));
        }
    }
}

fn wrap<'a>(allocator: &'a Allocator, filter: &VueFilterExpr<'a>) -> ExprRef<'a> {
    let out = wrap_source(filter);
    let text = allocator.alloc_str(out.as_str());
    ExprRef::parse_js_in(allocator, text, filter.span)
}

fn wrap_source(filter: &VueFilterExpr<'_>) -> String {
    let mut out = String::from(filter.base.source());
    for app in &filter.filters {
        out = wrap_one(out.as_str(), app);
    }
    out
}

fn wrap_one(exp: &str, app: &VueFilterApp<'_>) -> String {
    let id = asset_ident("filter", app.name);
    match app.raw.find('(') {
        None => {
            let mut out = String::with_capacity(id.len() + exp.len() + 2);
            out.push_str(id.as_str());
            out.push('(');
            out.push_str(exp);
            out.push(')');
            out
        }
        Some(idx) if &app.raw[idx + 1..] == ")" => {
            let mut out = String::with_capacity(id.len() + exp.len() + 2);
            out.push_str(id.as_str());
            out.push('(');
            out.push_str(exp);
            out.push(')');
            out
        }
        Some(idx) => {
            let args = &app.raw[idx + 1..];
            let mut out = String::with_capacity(id.len() + exp.len() + args.len() + 3);
            out.push_str(id.as_str());
            out.push('(');
            out.push_str(exp);
            out.push(',');
            out.push_str(args);
            out
        }
    }
}
