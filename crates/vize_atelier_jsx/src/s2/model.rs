use vize_relief::{DirectiveNode, ElementType};
use vize_s0::{Allocator, Box, Vec};
use vize_s1_to_s2::lower::{LoweringFeatures, OpFamily};
use vize_s2::op::{Attribute, BindingContract, BindingOp, ModelOp};

use super::{S2Refusal, lower_dynamic_name, lower_expression};

pub(super) fn lower_model<'a>(
    allocator: &'a Allocator,
    directive: &DirectiveNode<'a>,
    element_type: ElementType,
    features: &mut LoweringFeatures,
) -> Result<BindingOp<'a>, S2Refusal> {
    if element_type != ElementType::Component {
        return Err(S2Refusal::Directive);
    }
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
    Ok(BindingOp::Model(Box::new_in(
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
    )))
}
