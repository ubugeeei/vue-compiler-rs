use vize_s2::expr::ExprRef;
use vize_s2::op::{BindingOp, DynamicName, ElementOp, SlotContentOp};

use super::region_is_supported;

pub(super) fn slot_template_is_supported(element: &ElementOp<'_>) -> bool {
    element.attributes.is_empty()
        && matches!(
            element.bindings.as_slice(),
            [BindingOp::SlotContent(content)] if slot_content_is_supported(content)
        )
        && region_is_supported(&element.children)
}

pub(super) fn has_slot_content(bindings: &[BindingOp<'_>]) -> bool {
    bindings
        .iter()
        .any(|binding| matches!(binding, BindingOp::SlotContent(_)))
}

fn slot_content_is_supported(content: &SlotContentOp<'_>) -> bool {
    content
        .params
        .as_ref()
        .is_none_or(slot_params_are_supported)
        && content.modifiers.is_empty()
        && matches!(content.name, None | Some(DynamicName::Static(_)))
}

fn slot_params_are_supported(params: &ExprRef<'_>) -> bool {
    matches!(params, ExprRef::Js(_) | ExprRef::Opaque(_))
}
