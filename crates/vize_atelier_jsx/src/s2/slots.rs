use vize_relief::{DirectiveNode, ExpressionNode};
use vize_s0::{Allocator, Box, Span, Vec};
use vize_s2::op::{BindingOp, DynamicName, SlotContentOp};

use super::S2Refusal;

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
) -> Result<BindingOp<'a>, S2Refusal> {
    if directive.exp.is_some() || !directive.modifiers.is_empty() {
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

    Ok(BindingOp::SlotContent(Box::new_in(
        SlotContentOp {
            name,
            modifiers: Vec::new_in(&allocator),
            params: None,
            span: directive.loc.span,
        },
        &allocator,
    )))
}

fn cover_span(left: Span, right: Span) -> Span {
    Span::new(left.start.min(right.start), left.end.max(right.end))
}
