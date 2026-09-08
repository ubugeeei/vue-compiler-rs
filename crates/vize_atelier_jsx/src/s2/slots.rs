use std::vec::Vec as StdVec;

use vize_davinci::id::NodeId;
use vize_relief::{DirectiveNode, ExpressionNode};
use vize_s0::{Allocator, Box, Span, String, Vec};
use vize_s1_to_s2::lower::OpFamily;
use vize_s2::expr::ExprRef;
use vize_s2::op::{BindingOp, DynamicName, SlotContentOp};
use vize_s2::scope::{ScopeBinding, ScopeFacts, ScopeOrigin};

use super::{ProjectCx, S2Refusal, lower_expression, simple_identifier};

pub(super) fn has_slot_content(bindings: &[BindingOp<'_>]) -> bool {
    bindings
        .iter()
        .any(|binding| matches!(binding, BindingOp::SlotContent(_)))
}

pub(super) fn slot_template_span(base: Span, bindings: &[BindingOp<'_>]) -> Span {
    bindings.iter().fold(base, |span, binding| match binding {
        BindingOp::SlotContent(content) => cover_span(span, content.span),
        _ => span,
    })
}

pub(super) fn lower_slot_content<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
    node: Option<NodeId>,
    cx: &mut ProjectCx,
) -> Result<BindingOp<'a>, S2Refusal> {
    if !directive.modifiers.is_empty() {
        return Err(S2Refusal::Directive);
    }
    let name = match directive.arg.as_ref() {
        Some(ExpressionNode::Simple(simple)) if simple.is_static => {
            Some(DynamicName::Static(simple.content))
        }
        None => None,
        Some(ExpressionNode::Simple(_)) | Some(ExpressionNode::Compound(_)) => {
            return Err(S2Refusal::Directive);
        }
    };
    let params = directive
        .exp
        .as_ref()
        .map(|expression| lower_expression(allocator, expression))
        .transpose()?;
    if let Some(params) = &params {
        attach_slot_scope(cx, node, params);
    }
    cx.observe(OpFamily::SlotCarrier);

    Ok(BindingOp::SlotContent(Box::new_in(
        SlotContentOp {
            name,
            modifiers: Vec::new_in(&allocator),
            params,
            span: directive.loc.span,
        },
        &allocator,
    )))
}

fn attach_slot_scope(cx: &mut ProjectCx, node: Option<NodeId>, params: &ExprRef<'_>) {
    let tag = cx.mint_scope();
    let mut bindings = StdVec::new();
    if let Some(name) = simple_identifier(params) {
        bindings.push(ScopeBinding {
            name: String::from(name),
            origin: ScopeOrigin::Authored {
                span: params.span(),
            },
        });
    }
    cx.attach_scope(node, ScopeFacts { tag, bindings });
}

fn cover_span(left: Span, right: Span) -> Span {
    Span::new(left.start.min(right.start), left.end.max(right.end))
}
