use vize_s2::expr::ExprRef;
use vize_s2::op::{ForOp, IfBranch, IfOp, Op, Region};

use super::{
    component_is_supported, element_bindings_are_supported, has_slot_content, region_is_supported,
    slot_template_is_supported,
};

pub(super) fn if_is_supported(if_op: &IfOp<'_>) -> bool {
    !if_op.branches.is_empty() && if_op.branches.iter().all(if_branch_is_supported)
}

fn if_branch_is_supported(branch: &IfBranch<'_>) -> bool {
    branch
        .condition
        .as_ref()
        .is_none_or(expr_is_js_or_raw_emittable)
        && branch_region_is_supported(&branch.region)
}

pub(super) fn for_is_supported(for_op: &ForOp<'_>) -> bool {
    matches!(&for_op.binding.source, ExprRef::Js(_))
        && expr_is_js_or_raw_emittable(&for_op.binding.value)
        && for_op
            .binding
            .key
            .as_ref()
            .is_none_or(expr_is_js_or_raw_emittable)
        && for_op
            .binding
            .index
            .as_ref()
            .is_none_or(expr_is_js_or_raw_emittable)
        && branch_region_is_supported(&for_op.region)
}

fn branch_region_is_supported(region: &Region<'_>) -> bool {
    match region.ops.as_slice() {
        [Op::Element(element)] => {
            if element.tag == "template" && has_slot_content(&element.bindings) {
                return slot_template_is_supported(element);
            }
            element_bindings_are_supported(element) && region_is_supported(&element.children)
        }
        [Op::Component(component)] => component_is_supported(component),
        [Op::For(for_op)] => for_is_supported(for_op),
        _ => false,
    }
}

fn expr_is_js_or_raw_emittable(expr: &ExprRef<'_>) -> bool {
    matches!(expr, ExprRef::Js(_) | ExprRef::Opaque(_))
}
