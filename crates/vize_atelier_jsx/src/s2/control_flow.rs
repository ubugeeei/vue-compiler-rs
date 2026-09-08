use std::vec::Vec as StdVec;

use vize_davinci::id::NodeId;
use vize_relief::{ForNode, IfNode};
use vize_s0::{Allocator, Box, Span, String, Vec};
use vize_s1_to_s2::lower::OpFamily;
use vize_s2::expr::ExprRef;
use vize_s2::op::{ForBinding, ForOp, IfBranch, IfOp, Op, Region};
use vize_s2::scope::{ScopeBinding, ScopeFacts, ScopeOrigin};

use super::{ProjectCx, S2Refusal, lower_children, lower_expression};

pub(super) fn lower_if<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    if_node: &IfNode<'a>,
    cx: &mut ProjectCx,
) -> Result<Op<'a>, S2Refusal> {
    let mut branches = Vec::new_in(&allocator);
    for branch in &if_node.branches {
        let condition = branch
            .condition
            .as_ref()
            .map(|condition| lower_expression(allocator, condition))
            .transpose()?;
        let ops = lower_children(allocator, source, &branch.children, cx)?;
        branches.push(IfBranch {
            condition,
            region: Region { ops },
            span: branch.loc.span,
        });
    }
    cx.observe(OpFamily::If);
    Ok(Op::If(Box::new_in(
        IfOp {
            branches,
            span: if_node.loc.span,
        },
        &allocator,
    )))
}

pub(super) fn lower_for<'a>(
    allocator: &'a Allocator,
    source: &'a str,
    for_node: &ForNode<'a>,
    node: Option<NodeId>,
    cx: &mut ProjectCx,
) -> Result<Op<'a>, S2Refusal> {
    let source_expr = lower_expression(allocator, &for_node.source)?;
    let value = match &for_node.value_alias {
        Some(value) => lower_expression(allocator, value)?,
        None => empty_expression_at(allocator, source, for_node.source.loc().span.start),
    };
    let key = for_node
        .key_alias
        .as_ref()
        .map(|key| lower_expression(allocator, key))
        .transpose()?;
    let index = for_node
        .object_index_alias
        .as_ref()
        .map(|index| lower_expression(allocator, index))
        .transpose()?;
    let binding = ForBinding {
        source: source_expr,
        value,
        key,
        index,
    };
    attach_for_scope(cx, node, &binding);
    let ops = lower_children(allocator, source, &for_node.children, cx)?;
    cx.observe(OpFamily::For);
    Ok(Op::For(Box::new_in(
        ForOp {
            binding,
            region: Region { ops },
            span: for_node.loc.span,
        },
        &allocator,
    )))
}

fn empty_expression_at<'a>(allocator: &'a Allocator, source: &'a str, offset: u32) -> ExprRef<'a> {
    let start = usize::try_from(offset)
        .ok()
        .filter(|start| *start <= source.len())
        .unwrap_or(source.len());
    ExprRef::parse_js_in(allocator, &source[start..start], Span::new(offset, offset))
}

fn attach_for_scope(cx: &mut ProjectCx, node: Option<NodeId>, binding: &ForBinding<'_>) {
    let tag = cx.mint_scope();
    let mut bindings = StdVec::new();
    for expr in [
        Some(&binding.value),
        binding.key.as_ref(),
        binding.index.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(name) = simple_identifier(expr) {
            bindings.push(ScopeBinding {
                name: String::from(name),
                origin: ScopeOrigin::Authored { span: expr.span() },
            });
        }
    }
    cx.attach_scope(node, ScopeFacts { tag, bindings });
}

fn simple_identifier<'a>(expr: &ExprRef<'a>) -> Option<&'a str> {
    match expr {
        ExprRef::Js(js) => match js.ast {
            oxc_ast::ast::Expression::Identifier(_) => Some(js.source),
            _ => None,
        },
        ExprRef::Foreign(_) | ExprRef::Filter(_) | ExprRef::Opaque(_) => None,
    }
}
