//! Page-order id shift after `.sync` inserts a listener per expanded op.

use alloc::vec::Vec as StdVec;

use vize_davinci::id::NodeId;
use vize_davinci::side_table::SideTable;
use vize_s2::op::{BindingOp, Op};

use super::super::walk::{PageWalk, visit_ops};
use crate::lower::Lowered;

/// Page-order ids of every `vue.sync` binding, collected **before**
/// expansion so the shift `new = old + count(sync_id < old)` is exact.
pub(super) fn collect_sync_ids(ops: &[Op<'_>]) -> StdVec<NodeId> {
    let mut walk = PageWalk::new();
    let mut ids = StdVec::new();
    collect_ops(&mut walk, ops, &mut ids);
    ids
}

fn collect_ops(walk: &mut PageWalk, ops: &[Op<'_>], ids: &mut StdVec<NodeId>) {
    for op in ops {
        let _ = walk.mint();
        match op {
            Op::Element(element) => {
                collect_bindings(walk, &element.bindings, ids);
                collect_ops(walk, &element.children.ops, ids);
            }
            Op::Component(component) => {
                collect_bindings(walk, &component.bindings, ids);
                collect_ops(walk, &component.children.ops, ids);
            }
            Op::Slot(slot) => {
                collect_bindings(walk, &slot.bindings, ids);
                collect_ops(walk, &slot.fallback.ops, ids);
            }
            Op::If(if_op) => {
                for branch in &if_op.branches {
                    collect_ops(walk, &branch.region.ops, ids);
                }
            }
            Op::For(for_op) => collect_ops(walk, &for_op.region.ops, ids),
            Op::Text(_) | Op::Interpolation(_) | Op::Comment(_) => {}
        }
    }
}

fn collect_bindings(walk: &mut PageWalk, bindings: &[BindingOp<'_>], ids: &mut StdVec<NodeId>) {
    for binding in bindings {
        if let Some(id) = walk.mint()
            && matches!(binding, BindingOp::VueSync(_))
        {
            ids.push(id);
        }
    }
}

/// Rekey lowering side tables and provenance after `.sync` insertion.
pub(super) fn rekey(lowered: &mut Lowered<'_>, sync_ids: &[NodeId]) {
    if sync_ids.is_empty() {
        return;
    }
    rekey_table(&mut lowered.scopes, sync_ids);
    rekey_table(&mut lowered.texts, sync_ids);
    rekey_table(&mut lowered.if_facts, sync_ids);
    rekey_table(&mut lowered.for_facts, sync_ids);
    rekey_table(&mut lowered.wrappers, sync_ids);
    rekey_table(&mut lowered.for_wrappers, sync_ids);
    for record in &mut lowered.provenance {
        if let Some(id) = record.node {
            record.node = Some(shift(id, sync_ids));
        }
    }
}

fn rekey_table<T: Clone>(table: &mut SideTable<T>, sync_ids: &[NodeId]) {
    let entries: StdVec<(NodeId, T)> = table
        .sorted_entries()
        .into_iter()
        .map(|(id, value)| (id, value.clone()))
        .collect();
    table.clear();
    for (id, value) in entries {
        table.insert(shift(id, sync_ids), value);
    }
}

fn shift(old: NodeId, sync_ids: &[NodeId]) -> NodeId {
    let extra = sync_ids
        .iter()
        .filter(|id| id.index() < old.index())
        .count();
    let extra = u32::try_from(extra).unwrap_or(u32::MAX);
    NodeId::from_index(old.index().saturating_add(extra)).unwrap_or(old)
}

/// Re-derive `op_count` from the rewritten tree (bindings inserted).
pub(super) fn recount(lowered: &mut Lowered<'_>) {
    let mut walk = PageWalk::new();
    visit_ops(&mut walk, &mut lowered.root.ops, &mut |_, _| {});
    lowered.op_count = walk.minted();
}
