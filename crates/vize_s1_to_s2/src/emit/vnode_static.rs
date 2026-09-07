//! Static child vnode hoist gates for native element emission.

use vize_davinci::id::NodeId;
use vize_s0::ensure_sufficient_stack;
use vize_s2::op::{ElementOp, Namespace, Op};

use super::EmitCx;
use crate::pass::StaticLevel;

pub(super) fn should_hoist_static_children(
    cx: &EmitCx<'_>,
    element: &ElementOp<'_>,
    id: Option<NodeId>,
    allow_hoist: bool,
    branch_root: bool,
    for_item: bool,
) -> bool {
    if !cx.hoist_static {
        return false;
    }
    if cx.conditional_v_for_item {
        return false;
    }
    if branch_root && cx.template_if_branch_root && has_direct_interpolation_child(element) {
        if has_direct_component_child(element) {
            return true;
        }
        return false;
    }
    let requested =
        cx.hoist_static_vnodes || (allow_hoist && (branch_root || !element.bindings.is_empty()));
    if !requested {
        return false;
    }
    if branch_root || for_item {
        return true;
    }
    id.and_then(|id| cx.facts.static_facts.get(id))
        .is_some_and(|fact| fact.level == StaticLevel::NotStatic)
}

fn has_direct_interpolation_child(element: &ElementOp<'_>) -> bool {
    element
        .children
        .ops
        .iter()
        .any(|op| matches!(op, Op::Interpolation(_)))
}

fn has_direct_component_child(element: &ElementOp<'_>) -> bool {
    element
        .children
        .ops
        .iter()
        .any(|op| matches!(op, Op::Component(_)))
}

pub(super) fn can_whole_hoist_static_element(element: &ElementOp<'_>, is_ts: bool) -> bool {
    ensure_sufficient_stack(|| can_whole_hoist_static_element_guarded(element, is_ts))
}

fn can_whole_hoist_static_element_guarded(element: &ElementOp<'_>, is_ts: bool) -> bool {
    if element.namespace != Namespace::Html && element.tag == "svg" && !element.bindings.is_empty()
    {
        return false;
    }
    super::props_static::static_vnode_surface_can_hoist(
        &element.attributes,
        &element.bindings,
        is_ts,
    ) && element
        .children
        .ops
        .iter()
        .all(|op| can_whole_hoist_static_child(op, is_ts))
}

fn can_whole_hoist_static_child(op: &Op<'_>, is_ts: bool) -> bool {
    match op {
        Op::Text(_) => true,
        Op::Element(element) => {
            ensure_sufficient_stack(|| can_whole_hoist_static_element(element, is_ts))
        }
        _ => false,
    }
}
