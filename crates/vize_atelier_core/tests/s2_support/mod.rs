//! The P2-9 differential comparator: legacy transform lane vs the S2
//! passes, compared at the DOM-output level — the facts DOM codegen
//! consumes from an if structure (chain order, branch count and order,
//! condition text, branch keys — series 1) and from a for structure
//! (document order, source text, value/key/index alias texts — series
//! 2: `renderList`'s whole input surface; the iterated element's `key`
//! prop stays element surface in both lanes and is compared there by
//! neither, exactly as legacy codegen reads it per vnode). The
//! byte-level DOM comparison arrives when a DOM backend exists to emit
//! from S2 (P2-11); until then this projection is the strongest
//! output-determining oracle the transform lane has, and TS-11
//! (`corpus-diff --surface compiler`) holds the actual output bytes
//! still. Series 3 adds the slot projection — component slot grouping
//! (canonical names with their invented-vs-authored class, params
//! texts, group order) and outlet names — in the [`slots`] module.
//! Series 4 adds the text projection — the merged text-unit surface
//! (`createTextVNode` boundaries with their static/dynamic parts,
//! condensed text included) — in the [`text`] module. Series 5 adds
//! the binding-surface projection — per owner: static attributes,
//! `v-bind`/`v-on` units, custom directives, and the reconstructed
//! `v-model` contract — in the [`surface`] module, and turns the
//! dynamic-key, wrapper-key, and outlet-key skip classes into
//! comparisons.
//!
//! # Why this lives in test space (the dependency direction)
//!
//! `vize_atelier_core` is published; the Davinci crates are not, and
//! the release gate (`tests/tooling/moonbit-publish-crates.test.ts`)
//! rejects a published crate whose release graph names an unpublished
//! one. Dev-dependencies are stripped on publish, so the S2 lane and
//! this comparator ride dev-deps, never the compile path. The P1-7
//! in-`src` comparator shape does not apply here because the shipped
//! path has no migrated read yet: the S2 lane runs *beside* the legacy
//! lane, not inside it.
//!
//! # The lane flag (charter #26)
//!
//! `VIZE_DAVINCI_TRANSFORM=legacy` disarms the dual-run: the legacy
//! lane is then the only thing exercised, which is also the shipped
//! default. The plain witness pins non-zero comparison counts, so a
//! flag or cfg regression that silently disarms the lane fails loudly.
//!
//! # Skip classes are counted, never silent
//!
//! The two lanes parse with different S1 front ends, and the S1 v1
//! scope records deliberate tree deviations (no implied-end-tag
//! reconciliation, no entity decoding). The comparator therefore
//! compares exactly the domain both lanes claim to model — templates
//! neither lane **rejects** — and **counts** everything it declines:
//! legacy hard parse errors, S2 structural error diagnostics (evaluated
//! pre-pass, with lowering-published transform-equivalent diagnostics
//! admitted by the comparator; a malformed or expressionless `v-for`
//! skips here, matching the legacy transform's refusal to build a
//! `ForNode` from it), the legacy dynamic-argument
//! `:[key]` quirk, compound rebuilds of any expression position, the
//! slot projection's counted classes — conditional carriers, the
//! `v-slots` spread, filler-only implicit defaults ([`slots`] module
//! docs, series 3) — and the surface projection's counted classes
//! ([`surface`] module docs, series 5: still-deferred built-ins,
//! wrapper props, entity-bearing values, and pattern-scoped models;
//! dynamic-argument component models are now compared).
//! Recovery-level legacy notes (`ErrorCode::is_recovery` — spec repairs
//! such as self-closing rewrites the parser already applied) do **not**
//! skip: the first corpus run measured them on 3,027 of 12,021
//! templates, and comparing them held zero divergence, so excluding
//! them would have quietly shrunk the claim by a quarter. Divergence
//! inside the compared domain panics (TS-25): investigate, never
//! average.

#![allow(dead_code, unused_imports)] // each test binary uses a subset of this module

pub mod battery;
mod checks;
mod compare;
pub mod hoist;
pub mod hoist_old;
pub mod hoist_owner;
pub mod hoist_walk;
pub mod old_lane;
pub mod s2_lane;
pub mod slots;
pub mod slots_old;
pub mod surface;
pub mod surface_check;
pub mod surface_old;
pub mod surface_old_help;
pub mod surface_s2;
pub mod text;
pub mod text_old;

pub use battery::BATTERY;
pub use compare::compare;
pub use compare::compare_with;
pub use hoist::HoistCounters;
pub use slots::SlotCounters;
pub use surface::SurfaceCounters;
pub use text::TextCounters;

/// The comparator's process-global accounting, pinned exactly by the
/// plain witness and printed by the corpus entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counters {
    /// Templates handed to [`compare`].
    pub templates_seen: u64,
    /// Templates dual-run to completion with zero divergence.
    pub compared: u64,
    /// `VIZE_DAVINCI_TRANSFORM=legacy` disarmed the S2 lane.
    pub skipped_legacy_flag: u64,
    /// The legacy parser reported a **hard** error (recovery notes
    /// compare — see the module docs); outside both lanes' shared
    /// domain.
    pub skipped_old_parse_errors: u64,
    /// The S2 lowering reported an `Error` diagnostic (pre-pass).
    pub skipped_s2_errors: u64,
    /// `ui.if` ops compared.
    pub if_ops: u64,
    /// Branches compared.
    pub branches: u64,
    /// Static-key value comparisons that ran (carriers, outlets
    /// included since series 5).
    pub keys_static: u64,
    /// Dynamic-key text comparisons that ran (series 5 closed the
    /// deferral class of the same name).
    pub keys_dynamic: u64,
    /// `<template v-if>` wrapper keys compared (series 5 closed the
    /// installment-1 drop through the lowering's capture channel).
    pub keys_wrapper: u64,
    /// The legacy arg-content quirk: a dynamic-argument `:[key]` the
    /// legacy lane lifts as the branch key; S2 mirrors ordinary branch
    /// carriers and counts wrapper residuals.
    pub keys_dynamic_arg: u64,
    /// A legacy compound key rebuild: no single source text.
    pub keys_compound: u64,
    /// Old lane rebuilt a compound condition; no single source text to
    /// compare.
    pub conditions_compound: u64,
    /// `ui.for`s compared (series 2).
    pub for_ops: u64,
    /// Value-alias text comparisons that ran.
    pub for_values: u64,
    /// Key-alias text comparisons that ran.
    pub for_keys: u64,
    /// Index-alias text comparisons that ran.
    pub for_indexes: u64,
    /// Both lanes agreed the value alias is absent (`v-for=" in xs"`).
    pub for_values_absent: u64,
    /// Old lane rebuilt a compound source or alias; no single source
    /// text to compare.
    pub for_compound: u64,
    /// The slot half (series 3): units, groups, outlets, and the
    /// counted classes ([`slots`] module docs).
    pub slots: SlotCounters,
    /// The text half (series 4): units, parts, compounds, and the
    /// counted classes ([`text`] module docs).
    pub text: TextCounters,
    /// The binding-surface half (series 5): owners, attrs, binds, ons,
    /// directives, models, and the counted classes ([`surface`] module
    /// docs).
    pub surfaces: SurfaceCounters,
    /// The hoist-decision half (series 6): compared positions, agreed
    /// whole/props hoists, and the counted classes ([`hoist`] module
    /// docs).
    pub hoist: HoistCounters,
}
