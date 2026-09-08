//! Native HTML `ui.if` (`v-if` / `v-else-if` / `v-else`) emission.

use vize_davinci::id::NodeId;
use vize_s0::{Span, String, ToCompactString};
use vize_s2::expr::ExprRef;
use vize_s2::op::{IfBranch, IfOp, Op};

use super::EmitCx;
use super::EmitError;
use super::UnsupportedReason as Reason;
use super::buf::Buf;
use super::js::escape_js_string;
use super::prefix::Site;
use crate::lower::{BranchKeyKind, IfFacts};

pub(super) fn emit_if(
    cx: &mut EmitCx<'_>,
    if_op: &IfOp<'_>,
    id: Option<NodeId>,
) -> Result<(), EmitError> {
    if if_op.branches.is_empty() {
        return Err(EmitError::unsupported_at(
            Reason::IfWithoutBranches,
            if_op.span,
        ));
    }
    cx.buf.use_open_block();
    cx.buf.use_create_comment();
    let facts = id.and_then(|id| cx.facts.if_facts.get(id));
    for (i, branch) in if_op.branches.iter().enumerate() {
        let allocated = next_if_key(cx);
        if let Some(condition) = &branch.condition {
            if i == 0 {
                cx.buf.push("(");
                emit_condition(cx, condition, branch.span)?;
                cx.buf.push(")");
                cx.buf.indent();
                cx.buf.newline();
                cx.buf.push("? ");
            } else {
                cx.buf.newline();
                cx.buf.push(": (");
                emit_condition(cx, condition, branch.span)?;
                cx.buf.push(")");
                cx.buf.indent();
                cx.buf.newline();
                cx.buf.push("? ");
            }
        } else {
            cx.buf.newline();
            cx.buf.push(": ");
        }
        let key = branch_key_js(cx, facts, i, allocated)?;
        let saved = cx.if_branch_key;
        cx.if_branch_key = 0;
        let from_template = id
            .and_then(|id| cx.wrappers.get(id))
            .and_then(|keys| keys.from_template.get(i).copied())
            .unwrap_or(false);
        if from_template {
            super::tpl::emit_if_template_branch(cx, branch, key.as_str())?;
        } else {
            emit_branch(cx, branch, key.as_str())?;
        }
        cx.if_branch_key = saved;
        if branch.condition.is_some() && i > 0 {
            cx.buf.deindent();
        }
    }
    if if_op
        .branches
        .iter()
        .all(|branch| branch.condition.is_some())
    {
        cx.buf.newline();
        cx.buf.push(": ");
        cx.buf.push(Buf::create_comment_alias());
        cx.buf.push("(\"v-if\", true)");
    }
    cx.buf.deindent();
    Ok(())
}

fn next_if_key(cx: &mut EmitCx<'_>) -> u32 {
    let key = cx.if_branch_key;
    cx.if_branch_key = cx.if_branch_key.saturating_add(1);
    key
}

fn emit_condition(
    cx: &mut EmitCx<'_>,
    condition: &ExprRef<'_>,
    branch_span: Span,
) -> Result<(), EmitError> {
    if cx.prefixing() {
        return cx.push_prefixed_expr(condition, Site::Expression);
    }
    match condition {
        ExprRef::Js(js) => {
            let source = super::js::js_expr_source(js);
            if let Some((leading, trailing)) =
                authored_condition_padding(cx.source, branch_span, source.as_str(), js.span)
                    .or_else(|| {
                        authored_condition_quote_padding(cx.source, source.as_str(), js.span)
                    })
                    .or_else(|| {
                        authored_condition_padding(cx.source, branch_span, js.source, js.span)
                    })
                    .or_else(|| authored_condition_quote_padding(cx.source, js.source, js.span))
            {
                cx.buf.push(leading);
                cx.buf.push(source.as_str());
                cx.buf.push(trailing);
            } else {
                cx.buf.push(source.as_str());
            }
            Ok(())
        }
        _ => {
            if let Some(raw) = super::js::parse_rejected_raw_js(condition, false) {
                if let Some((leading, trailing)) = authored_condition_padding(
                    cx.source,
                    branch_span,
                    raw.as_str(),
                    condition.span(),
                ) {
                    cx.buf.push(leading);
                    cx.buf.push(raw.as_str());
                    cx.buf.push(trailing);
                } else {
                    cx.buf.push(raw.as_str());
                }
                Ok(())
            } else {
                Err(EmitError::unsupported_at(
                    Reason::IfConditionNotJs,
                    condition.span(),
                ))
            }
        }
    }
}

fn authored_condition_padding<'a>(
    source: &'a str,
    owner_span: Span,
    value: &str,
    value_span: Span,
) -> Option<(&'a str, &'a str)> {
    let attr_start = usize::try_from(owner_span.start).ok()?;
    let attr_end = usize::try_from(owner_span.end).ok()?;
    let value_start = usize::try_from(value_span.start).ok()?;
    let value_end = usize::try_from(value_span.end).ok()?;
    if attr_start > value_start
        || value_start > value_end
        || value_end > attr_end
        || attr_end > source.len()
        || source.get(value_start..value_end)? != value
    {
        return None;
    }
    let before = source.get(attr_start..value_start)?;
    let quote_pos = before
        .as_bytes()
        .iter()
        .rposition(|byte| matches!(*byte, b'\'' | b'"'))?;
    let quote = before.as_bytes()[quote_pos];
    let leading = before.get(quote_pos + 1..)?;
    let after = source.get(value_end..attr_end)?;
    let trailing_end = after
        .as_bytes()
        .iter()
        .position(|byte| *byte == quote)
        .unwrap_or(after.len());
    let trailing = after.get(..trailing_end)?;
    if leading.is_empty() && trailing.is_empty() {
        return None;
    }
    (leading.bytes().all(|byte| byte.is_ascii_whitespace())
        && trailing.bytes().all(|byte| byte.is_ascii_whitespace()))
    .then_some((leading, trailing))
}

fn authored_condition_quote_padding<'a>(
    source: &'a str,
    value: &str,
    value_span: Span,
) -> Option<(&'a str, &'a str)> {
    let value_start = usize::try_from(value_span.start).ok()?;
    let value_end = usize::try_from(value_span.end).ok()?;
    if value_start > value_end
        || value_end > source.len()
        || source.get(value_start..value_end)? != value
    {
        return None;
    }
    let before = source.get(..value_start)?;
    let quote_pos = before
        .as_bytes()
        .iter()
        .rposition(|byte| matches!(*byte, b'\'' | b'"'))?;
    let quote = before.as_bytes()[quote_pos];
    let leading = source.get(quote_pos + 1..value_start)?;
    let after = source.get(value_end..)?;
    let trailing_end = after.as_bytes().iter().position(|byte| *byte == quote)?;
    let trailing = after.get(..trailing_end)?;
    if leading.is_empty() && trailing.is_empty() {
        return None;
    }
    (leading.bytes().all(|byte| byte.is_ascii_whitespace())
        && trailing.bytes().all(|byte| byte.is_ascii_whitespace()))
    .then_some((leading, trailing))
}

fn branch_key_js(
    cx: &EmitCx<'_>,
    facts: Option<&IfFacts>,
    index: usize,
    allocated: u32,
) -> Result<String, EmitError> {
    match facts
        .and_then(|facts| facts.branches.get(index))
        .and_then(|key| key.as_ref())
        .map(|key| &key.kind)
    {
        None | Some(BranchKeyKind::Static(None)) => Ok(allocated.to_compact_string()),
        Some(BranchKeyKind::Static(Some(value))) => {
            let mut out = String::from("\"");
            out.push_str(escape_js_string(value.as_str()).as_str());
            out.push('"');
            Ok(out)
        }
        Some(BranchKeyKind::Dynamic { source, .. }) if source.is_empty() => {
            Ok(allocated.to_compact_string())
        }
        Some(BranchKeyKind::Dynamic { source, .. }) if cx.prefixing() => {
            cx.prefixed_text(source.as_str(), Site::Expression)
        }
        Some(BranchKeyKind::Dynamic { source, .. }) => Ok(source.clone()),
    }
}

fn emit_branch(cx: &mut EmitCx<'_>, branch: &IfBranch<'_>, key: &str) -> Result<(), EmitError> {
    match branch.region.ops.as_slice() {
        [Op::Element(element)] => {
            let _id = cx.walk.mint();
            cx.walk.skip(element.bindings.len());
            super::emit_if_branch_call(cx, element, key)
        }
        [Op::Component(component)] => {
            let _id = cx.walk.mint();
            cx.walk.skip(component.bindings.len());
            let previous = cx.template_if_branch_root;
            cx.template_if_branch_root = authored_template_branch(cx, branch);
            let result = super::component::emit_if_branch(cx, component, key, _id);
            cx.template_if_branch_root = previous;
            result
        }
        [Op::Slot(slot)] => {
            let _id = cx.walk.mint();
            cx.walk.skip(slot.bindings.len());
            super::outlet::emit_outlet(cx, slot, Some(key), true)
        }
        [Op::For(for_op)] => {
            let id = cx.walk.mint();
            super::emit_for_op(cx, for_op, id, Some(key))
        }
        _ => Err(EmitError::unsupported_at(
            Reason::IfBranchShape,
            branch.span,
        )),
    }
}

fn authored_template_branch(cx: &EmitCx<'_>, branch: &IfBranch<'_>) -> bool {
    let Ok(start) = usize::try_from(branch.span.start) else {
        return false;
    };
    let Ok(end) = usize::try_from(branch.span.end) else {
        return false;
    };
    cx.source
        .get(start..end)
        .is_some_and(|source| source.trim_start().starts_with("<template"))
}
