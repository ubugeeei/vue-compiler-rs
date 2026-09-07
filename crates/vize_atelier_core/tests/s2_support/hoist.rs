//! The hoist-decision projection (P2-9 series 6): the shipped
//! `hoist_static` decisions — whole-vnode hoists and props hoists,
//! read from a hoist-armed legacy run's actual mutations — against the
//! predictions of the S2 analysis facts (`StaticFacts`) driven through
//! the shipped decision procedure's position rules. Decisions, never
//! realization bytes: what hoists is compared, what the hoisted
//! `VNodeCall` looks like is P2-11's parity bar.
//!
//! # Replay control is legacy ground truth
//!
//! The walk ([`super::hoist_walk`]) descends exactly where the shipped
//! driver descended, by re-asking the shipped predicate
//! (`get_static_type`, exported) on the unmutated run-1 tree — so a
//! fact divergence can never desynchronize the traversal; it surfaces
//! as a verdict mismatch at one aligned position. The S2 facts are
//! used **only** as the comparison target, through [`predict`] — the
//! shipped position rules over the published facts.
//!
//! # Counted classes, never silence
//!
//! Template-level (skipped whole, one count each — detectors in
//! `mod.rs` / [`super::hoist_old`]): `vpre_templates`,
//! `table_templates`, `models_templates` (legacy removes a model S2
//! only faults, or the pattern-scope seam makes the removal verdicts
//! incomparable), `classifier_templates` (the lanes disagree on
//! element-vs-component, flipping the lattice and the vnodes flag in
//! both directions), `consts_templates` (the S2 const rule is
//! deliberately weaker — the legacy lane may hoist *more*, gone
//! subtrees included), and `tree_templates` (the shape pre-check
//! failed: the S1 v1 no-reconciliation scope's face on nesting;
//! in-table construction is counted separately first).
//!
//! Element-level: `comments_elements` — a direct comment child blocks
//! the legacy lattice and the nested-child class while S2 is
//! comment-blind; the element's own verdict (and its ancestors', via
//! taint propagation) is skipped, its descendants still compare —
//! sound because a comment-bearing element is legacy-`NotStatic`, so
//! the shipped driver demonstrably descended, with a flag both lanes
//! agree on. Subtree-level: `builtins_subtrees` — a deferred builtin
//! poisons the inherited vnodes flag below its element, so the whole
//! subtree's verdicts are suppressed (walked for alignment only).
//!
//! Taint propagates **up through element parents only**: an `If`/`For`
//! child makes its parent dynamic in both lanes regardless, a
//! component/outlet child contributes fixed level values, so their
//! interiors cannot move any ancestor's fact — the boundary keeps
//! corpus comparisons alive around tainted islands.

use vize_atelier_core::TemplateChildNode;
use vize_davinci::id::NodeId;
use vize_davinci::side_table::SideTable;
use vize_s1_to_s2::pass::{StaticFacts, StaticLevel};
use vize_s2::folio::FolioOp;

use super::hoist_old::Decision;
use super::hoist_walk::{structural, walk_level};

/// The hoist half's accounting, part of [`super::Counters`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HoistCounters {
    /// Element/component/outlet positions whose verdict compared.
    pub elements: u64,
    /// Agreed whole-vnode hoists.
    pub whole: u64,
    /// Agreed props hoists.
    pub props: u64,
    /// `<template v-for>` wrapper positions the legacy lane
    /// props-hoisted (S2 keeps no wrapper position; counted, never
    /// compared).
    pub wrapper_hoists: u64,
    /// Elements whose verdict was skipped for a direct comment child.
    pub comments_elements: u64,
    /// Subtrees suppressed under a deferred-builtin carrier.
    pub builtins_subtrees: u64,
    /// The shipped const classifier admitted a value the S2 rule
    /// refuses (module docs) — whole template counted.
    pub consts_templates: u64,
    /// The lanes classify an owner differently (module docs).
    pub classifier_templates: u64,
    /// The model-removal verdicts are not comparable (module docs).
    pub models_templates: u64,
    /// The two S1 trees nest differently (module docs).
    pub tree_templates: u64,
    /// **Retired**, kept at zero: the `v-pre` deferral it counted is
    /// gone (see [`super::text`]'s note), so these owners are replayed
    /// like any other.
    pub vpre_templates: u64,
    /// The legacy in-table tree-construction class.
    pub table_templates: u64,
}

/// The walk mode: `Replay` follows an arm the shipped driver actually
/// took (with its position flags); `Dormant` covers regions the driver
/// never descended into — no decision can exist there, which the walk
/// still asserts position by position.
#[derive(Clone, Copy)]
pub enum Mode {
    /// Mirroring an arm the shipped driver took.
    Replay {
        /// The driver's root-level flag.
        is_root: bool,
        /// The inherited `hoist_static_vnodes` flag.
        vnodes: bool,
        /// Single v-for item: the VNode stays a block; static props may hoist.
        for_item: bool,
    },
    /// A region the driver never entered.
    Dormant,
}

/// Replay with new flags, unless the walk is already dormant.
pub fn replay_or_dormant(mode: Mode, is_root: bool, vnodes: bool) -> Mode {
    match mode {
        Mode::Replay { .. } => Mode::Replay {
            is_root,
            vnodes,
            for_item: false,
        },
        Mode::Dormant => Mode::Dormant,
    }
}

/// The `hoist_for_children` single-item arm: props only, never a whole vnode.
pub fn replay_for_item(mode: Mode) -> Mode {
    match mode {
        Mode::Replay { .. } => Mode::Replay {
            is_root: false,
            vnodes: true,
            for_item: true,
        },
        Mode::Dormant => Mode::Dormant,
    }
}

/// Mirror `hoist_for_children`: a single element item hoists props only
/// (the VNode is the loop block); anything else uses the vnodes walk.
#[expect(clippy::too_many_arguments, reason = "one recursive comparator walk")]
pub fn walk_for_body(
    name: &str,
    source: &str,
    old1: &[TemplateChildNode<'_>],
    old2: &[TemplateChildNode<'_>],
    s2: &[FolioOp],
    mode: Mode,
    suppressed: bool,
    next: &mut u32,
    facts: &SideTable<StaticFacts>,
    counters: &mut HoistCounters,
) {
    match structural(old1).as_slice() {
        [TemplateChildNode::Element(el1)] if el1.tag == "template" => {
            let o2 = structural(old2);
            let [TemplateChildNode::Element(el2)] = o2.as_slice() else {
                panic!("template v-for children misaligned in {name}\n{source}");
            };
            if el2.hoisted_props_index.is_some() {
                counters.wrapper_hoists += 1;
            }
            walk_for_body(
                name,
                source,
                &el1.children,
                &el2.children,
                s2,
                mode,
                suppressed,
                next,
                facts,
                counters,
            );
        }
        [TemplateChildNode::Element(_)] => {
            walk_level(
                name,
                source,
                old1,
                old2,
                s2,
                replay_for_item(mode),
                suppressed,
                next,
                facts,
                counters,
            );
        }
        _ => {
            walk_level(
                name,
                source,
                old1,
                old2,
                s2,
                replay_or_dormant(mode, false, true),
                suppressed,
                next,
                facts,
                counters,
            );
        }
    }
}

/// Compare one template's decisions. `old1` is the default (unmutated)
/// run's children, `old2` the hoist-armed run's, `s2` the folio ops.
///
/// # Panics
///
/// Panics on any divergence inside the compared domain (TS-25), with
/// the template and the diverging position's context in the message.
pub fn check(
    name: &str,
    source: &str,
    old1: &[TemplateChildNode<'_>],
    old2: &[TemplateChildNode<'_>],
    s2: &[FolioOp],
    facts: &SideTable<StaticFacts>,
    counters: &mut HoistCounters,
) {
    let mut next = 0u32;
    let _ = walk_level(
        name,
        source,
        old1,
        old2,
        s2,
        Mode::Replay {
            is_root: true,
            vnodes: false,
            for_item: false,
        },
        false,
        &mut next,
        facts,
        counters,
    );
}

/// The S2 tree's shape projection — the pairing contract's S2 half
/// (byte-compared against [`super::hoist_old::shape_of`] before any
/// walk; a mismatch is `tree_templates`).
pub fn shape_of_s2(ops: &[FolioOp], out: &mut vize_s0::String) {
    for op in ops {
        match op {
            FolioOp::Element(element) => {
                out.push('e');
                out.push('(');
                shape_of_s2(&element.children, out);
                out.push(')');
            }
            FolioOp::Component(component) => {
                out.push('c');
                out.push('(');
                shape_of_s2(&component.children, out);
                out.push(')');
            }
            FolioOp::Slot(slot) => {
                out.push('s');
                out.push('(');
                shape_of_s2(&slot.fallback, out);
                out.push(')');
            }
            FolioOp::If(if_op) => {
                out.push('i');
                for branch in if_op.branches.iter() {
                    out.push('[');
                    shape_of_s2(&branch.ops, out);
                    out.push(']');
                }
            }
            FolioOp::For(for_op) => {
                out.push('f');
                out.push('(');
                shape_of_s2(&for_op.ops, out);
                out.push(')');
            }
            FolioOp::Text(_) | FolioOp::Interpolation(_) | FolioOp::Comment(_) => {}
        }
    }
}

/// `hoist_for_children` on a single item: static props hoist, VNode stays inline.
pub fn predict_for_item(fact: &StaticFacts) -> Decision {
    if fact.props_hoistable {
        Decision::Props
    } else {
        Decision::None
    }
}

/// The shipped decision procedure over the facts (the position rules
/// of `hoist_static_inner`, options at the comparator's defaults:
/// `inline` off, so that root arm is dead here and its predicate is
/// battery-covered in ricalco's own suites; `scope_id` shapes the
/// hoisted payload, never the decision). The `ns != Html` arm reads
/// the fact's own `foreign` bit — the analysis carries the namespace
/// context precisely because `ui.component` has none of its own.
pub fn predict(fact: &StaticFacts, is_root: bool, vnodes: bool) -> Decision {
    let foreign = fact.foreign;
    match fact.level {
        StaticLevel::FullyStatic => {
            if is_root && fact.props_hoistable {
                Decision::Props
            } else if vnodes {
                Decision::Whole
            } else {
                Decision::None
            }
        }
        StaticLevel::HasDynamicText => {
            if is_root && fact.props_hoistable {
                Decision::Props
            } else {
                Decision::None
            }
        }
        StaticLevel::NotStatic => {
            if fact.props_hoistable && (foreign || fact.nested_static) {
                Decision::Props
            } else {
                Decision::None
            }
        }
    }
}

/// The dense-facts lookup: every element/component owner carries one.
pub fn fact_of<'t>(
    facts: &'t SideTable<StaticFacts>,
    owner: u32,
    name: &str,
    source: &str,
) -> &'t StaticFacts {
    NodeId::from_index(owner)
        .and_then(|id| facts.get(id))
        .unwrap_or_else(|| panic!("owner {owner} has no static fact in {name}\n{source}"))
}
