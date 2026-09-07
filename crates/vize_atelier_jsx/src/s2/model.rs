use vize_relief::{DirectiveNode, ElementType, ExpressionNode};
use vize_s0::{Allocator, Box, Vec};
use vize_s1_to_s2::lower::{LoweringFeatures, OpFamily};
use vize_s2::{
    expr::ExprRef,
    op::{Attribute, BindingContract, BindingOp, DynamicName, ModelOp},
};

use super::{S2Refusal, lower_dynamic_name, lower_expression};

pub(super) fn lower_model<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
    element_type: ElementType,
    native_text_model_kind: Option<&'a str>,
    features: &mut LoweringFeatures,
) -> Result<BindingOp<'a>, S2Refusal> {
    match element_type {
        ElementType::Component => lower_component_model(allocator, directive, features),
        ElementType::Element => {
            let Some(element_kind) = native_text_model_kind else {
                return Err(S2Refusal::Directive);
            };
            lower_native_text_model(allocator, directive, element_kind, features)
        }
        _ => Err(S2Refusal::Directive),
    }
}

fn lower_component_model<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
    features: &mut LoweringFeatures,
) -> Result<BindingOp<'a>, S2Refusal> {
    let Some(expression) = directive.exp.as_ref() else {
        return Err(S2Refusal::Directive);
    };

    let value = lower_expression(allocator, expression)?;
    let argument = lower_dynamic_name(allocator, directive.arg.as_ref())?;
    let mut attributes = Vec::new_in(&allocator);
    attributes.push(Attribute {
        name: "element-kind",
        value: Some("component"),
        span: directive.loc.span,
    });
    for modifier in &directive.modifiers {
        attributes.push(Attribute {
            name: modifier.content,
            value: None,
            span: modifier.loc.span,
        });
    }

    *features = features.observing(OpFamily::Model);
    Ok(model_op(allocator, directive, value, argument, attributes))
}

fn lower_native_text_model<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
    element_kind: &'a str,
    features: &mut LoweringFeatures,
) -> Result<BindingOp<'a>, S2Refusal> {
    if directive.arg.is_some() || !directive.modifiers.is_empty() {
        return Err(S2Refusal::Directive);
    }
    let Some(expression) = directive.exp.as_ref() else {
        return Err(S2Refusal::Directive);
    };
    if is_jsx_model_tuple(expression) {
        return Err(S2Refusal::Directive);
    }

    let value = lower_expression(allocator, expression)?;
    let mut attributes = Vec::new_in(&allocator);
    attributes.push(Attribute {
        name: "element-kind",
        value: Some(element_kind),
        span: directive.loc.span,
    });

    *features = features.observing(OpFamily::Model);
    Ok(model_op(allocator, directive, value, None, attributes))
}

fn is_jsx_model_tuple(expression: &ExpressionNode<'_>) -> bool {
    matches!(expression, ExpressionNode::Simple(simple) if simple.content.trim_start().starts_with('['))
}

fn model_op<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
    value: ExprRef<'a>,
    argument: Option<DynamicName<'a>>,
    attributes: Vec<'a, Attribute<'a>>,
) -> BindingOp<'a> {
    BindingOp::Model(Box::new_in(
        ModelOp {
            contract: BindingContract {
                read: value,
                write: value,
            },
            argument,
            attributes,
            span: directive.loc.span,
        },
        &allocator,
    ))
}
