use vize_relief::{DirectiveNode, ElementType};
use vize_s0::{Allocator, Box};
use vize_s2::expr::ExprRef;
use vize_s2::op::{BindingOp, VueHtmlOp, VueShowOp, VueTextOp};

use super::{S2Refusal, lower_expression};

pub(super) fn lower_vue_directive<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
    element_type: ElementType,
) -> Result<BindingOp<'a>, S2Refusal> {
    if directive.arg.is_some() || !directive.modifiers.is_empty() {
        return Err(S2Refusal::Directive);
    }

    match directive.name {
        "show" => lower_show(allocator, directive),
        "html" if element_type == ElementType::Element => lower_html(allocator, directive),
        "text" if element_type == ElementType::Element => lower_text(allocator, directive),
        _ => Err(S2Refusal::Directive),
    }
}

fn lower_show<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
) -> Result<BindingOp<'a>, S2Refusal> {
    let value = required_value(allocator, directive)?;

    Ok(BindingOp::VueShow(Box::new_in(
        VueShowOp {
            value,
            span: directive.loc.span,
        },
        &allocator,
    )))
}

fn lower_html<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
) -> Result<BindingOp<'a>, S2Refusal> {
    let value = required_value(allocator, directive)?;

    Ok(BindingOp::VueHtml(Box::new_in(
        VueHtmlOp {
            value: Some(value),
            span: directive.loc.span,
        },
        &allocator,
    )))
}

fn lower_text<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
) -> Result<BindingOp<'a>, S2Refusal> {
    let value = required_value(allocator, directive)?;

    Ok(BindingOp::VueText(Box::new_in(
        VueTextOp {
            value: Some(value),
            span: directive.loc.span,
        },
        &allocator,
    )))
}

fn required_value<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
) -> Result<ExprRef<'a>, S2Refusal> {
    let Some(expression) = directive.exp.as_ref() else {
        return Err(S2Refusal::Directive);
    };
    lower_expression(allocator, expression)
}
