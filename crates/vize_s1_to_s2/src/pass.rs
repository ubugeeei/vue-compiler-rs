//! The S2 transform passes over lowered Vue output — the P2-9 series
//! substrate.
//!
//! # Why the passes live here (the dependency-direction decision)
//!
//! P2-9 re-expresses `vize_atelier_core`'s transform lane as classified
//! S2 passes, but the passes cannot live *in* `vize_atelier_core`: that
//! crate is published to crates.io and the release gate
//! (`tests/tooling/moonbit-publish-crates.test.ts`) rejects any
//! published crate whose release graph names an unpublished one — the
//! constraint `davinci-road/plan/phase-2.md` records under "Davinci
//! describes the shipped pipeline". The backends read Davinci from
//! **dev-dependencies** (stripped on publish), which is where the P2-9
//! differential comparator sits (`crates/vize_atelier_core/tests/
//! davinci_s2_transform*.rs`); the pass bodies themselves need a crate
//! that may depend on `vize_davinci` + `vize_s2` outright.
//!
//! That crate is this one. `vize_s1_to_s2` is `publish = false`, already
//! depends downward on both stages, and the passes are the continuation
//! of the dialect lowering — the MLIR conversion-library shape extended
//! one step: `lower` converts, the passes legalize. What lowering leaves
//! syntactic (a branch `key` attribute, a deferred binding) a pass here
//! turns semantic, dialect knowledge included, without the neutral pivot
//! (`vize_s2`) learning any Vue and without the published legacy
//! lane gaining an edge on the strangler's new lane.
//!
//! # The in-phase lane flag (charter #26)
//!
//! [`TRANSFORM_LANE_FLAG`] names the env flag the P2-9 series runs
//! behind: `VIZE_DAVINCI_TRANSFORM=legacy` disarms the S2 dual-run in
//! the differential comparator, leaving the shipped legacy lane alone as
//! today. The flag is *read* in `vize_atelier_core`'s test-space
//! comparator (this crate is `no_std` and reads no environment); it is
//! *named* here so the phase-2 exit gate's deletion grep has one home.
//! The old lane is the only shipped lane while the phase is live; the
//! differential lane, not the flag, carries the risk (phase-2.md § 11).
//!
//! # Driving a run
//!
//! [`run_transform`] executes the artifact-selected S2 plan through the
//! P2-2 pass manager (`vize_davinci::pass::run_pipeline`) with a
//! caller-supplied observer, and wires the P2-6 [`VerifyObserver`] between
//! passes in debug builds exactly as its module documents: `note` then
//! `check` / `check_table` after every pass, so a broken invariant names
//! the pass that broke it. Release builds make zero verifier calls
//! (guardrail 5).

use vize_davinci::pass::{PassDesc, PassFailure, PassObserver, Pipeline, run_pipeline};
use vize_davinci::side_table::SideTable;

use crate::lower::Lowered;

pub mod hoist;
pub mod legacy;
mod plan;
pub mod text;
pub mod vfor;
pub mod vif;
pub mod vmodel;
pub mod vslot;
pub(crate) mod walk;

pub use hoist::{StaticFacts, StaticLevel};
pub use plan::TransformProfile;
pub use text::TextFacts;
pub use vfor::{ForFacts, ForName};
pub use vif::{BranchKey, BranchKeyKind, IfFacts};
pub use vmodel::{ModelFacts, ModelFault};
pub use vslot::{SlotCarrier, SlotFacts, SlotGroup, SlotName, SlotParams};

/// The stage name S2 transform pipelines print and parse under.
pub const S2_STAGE: &str = "s2";

/// The env flag the P2-9 series runs behind (charter #26): value
/// `legacy` disarms the S2 dual-run lane. Read by the differential
/// comparator in `vize_atelier_core`'s test space; deleted, with the
/// lane it guards, at the phase-2 exit gate.
pub const TRANSFORM_LANE_FLAG: &str = "VIZE_DAVINCI_TRANSFORM";

/// The S2 transform pipeline as the series has built it so far.
///
/// Six passes ([`vif::DESC`], [`vfor::DESC`], [`vslot::DESC`],
/// [`text::DESC`], [`vmodel::DESC`], then [`hoist::DESC`] — the
/// series' landing order; the passes touch disjoint op families, the
/// model pass preserves, and the analysis pass mutates nothing, so the
/// order still carries no semantic dependency today). Later
/// installments append here, and the `const` pins below are the
/// grouping regression guard (the P2-2 convention: a fusion-plan change
/// is a compile error, not a surprise).
pub const TRANSFORM_PASSES: &[PassDesc] = &[
    vif::DESC,
    vfor::DESC,
    vslot::DESC,
    text::DESC,
    vmodel::DESC,
    hoist::DESC,
];

/// The planned pipeline over [`TRANSFORM_PASSES`].
pub const TRANSFORM: Pipeline = Pipeline::new(S2_STAGE, TRANSFORM_PASSES);

// The plan's fusion shape, pinned: five mandatory barriers plus the
// series' first `Optional`/`Fusable` pass (installment 6, the
// hoist-static analysis). What the fusion machinery actually does with
// it, measured and pinned: the pass forms the pipeline's first
// NON-BARRIER group — a singleton, because grouping starts fresh after
// a barrier and no fusable neighbour exists yet — so `group_count()`
// rises to 6 and `is_fully_serialized()` stays true in its literal
// sense (fusion still buys nothing: six groups for six passes). The
// door the P2-2 laws hold open is now real: the next fusable pass to
// land adjacent joins this group and drops the walk count below the
// pass count for the first time.
const _: () = assert!(TRANSFORM.group_count() == 6);
const _: () = assert!(TRANSFORM.is_fully_serialized());
const _: () = {
    let group = match TRANSFORM.group(5) {
        Some(group) => group,
        None => panic!("the sixth group exists"),
    };
    assert!(group.start == 5 && group.len == 1 && !group.is_barrier);
};

/// The facts the S2 transform pipeline produces beside the tree.
///
/// One field per fact family, so a later installment's facts land as new
/// fields rather than a second bag type.
#[derive(Debug, Default)]
pub struct S2Facts {
    /// Facts from the Vue 2 sugar-legalizing pass, empty on Vue 3.
    pub legacy: legacy::LegacyFacts,
    /// Per-`ui.if` branch-key facts, keyed by the op's page-order id
    /// ([`vif`]).
    pub if_facts: SideTable<IfFacts>,
    /// Per-`ui.for` consumed-scope facts, keyed by the op's page-order
    /// id ([`vfor`]).
    pub for_facts: SideTable<ForFacts>,
    /// Per-`ui.component` canonical slot grouping, keyed by the op's
    /// page-order id ([`vslot`]).
    pub slot_facts: SideTable<SlotFacts>,
    /// Per-compound merged-run parts, keyed by the compound
    /// `ui.interpolation` op's page-order id ([`text`]).
    pub text_facts: SideTable<TextFacts>,
    /// Per-`ui.model` validation faults, keyed by the binding op's
    /// page-order id ([`vmodel`]); sparse — entries only for models the
    /// legacy lane would remove.
    pub model_faults: SideTable<ModelFacts>,
    /// Per-owner static-analysis facts, keyed by the `ui.element` /
    /// `ui.component` op's page-order id ([`hoist`]); dense over the
    /// owner family. The series' first `Optional` product: skipping the
    /// pass loses these and nothing else.
    pub static_facts: SideTable<StaticFacts>,
}

/// Run the artifact-selected S2 transform pipeline over `lowered`, firing
/// `observer`'s hooks around each pass.
///
/// Pass diagnostics and provenance append to `lowered`'s own channels —
/// the unified-channel design; there is no second diagnostics stream.
/// In debug builds the S2 verifier runs between passes ([`VerifyObserver`]
/// wiring per its docs); a violated invariant panics naming the pass.
///
/// # Panics
///
/// Panics only on a compiler bug: a pipeline pass with no registered
/// body, or (debug builds) a verifier violation.
///
/// [`VerifyObserver`]: vize_s2::verify::VerifyObserver
pub fn run_transform<'a, O: PassObserver>(lowered: &mut Lowered<'a>, observer: &mut O) -> S2Facts {
    run_transform_with_profile(lowered, observer, TransformProfile::DEFAULT)
}

/// [`run_transform`] under a product-selected optional-pass profile.
pub fn run_transform_with_profile<'a, O: PassObserver>(
    lowered: &mut Lowered<'a>,
    observer: &mut O,
    profile: TransformProfile,
) -> S2Facts {
    let mut facts = S2Facts::default();
    #[cfg(debug_assertions)]
    let mut verify = vize_s2::verify::VerifyObserver::new();

    let pipeline = plan::pipeline_for_profile(lowered.caps, lowered.features, profile);
    let outcome = run_pipeline(&pipeline, observer, |event| {
        let name = event.desc().name;
        if name == legacy::DESC.name {
            facts.legacy = legacy::run(lowered);
        } else if name == vif::DESC.name {
            facts.if_facts = vif::run(lowered);
        } else if name == vfor::DESC.name {
            facts.for_facts = vfor::run(lowered);
        } else if name == vslot::DESC.name {
            facts.slot_facts = vslot::run(lowered);
        } else if name == text::DESC.name {
            facts.text_facts = text::run(lowered);
        } else if name == vmodel::DESC.name {
            facts.model_faults = vmodel::run(lowered);
        } else if name == hoist::DESC.name {
            facts.static_facts = hoist::run(lowered);
        } else {
            return Err(PassFailure::new("pipeline pass has no registered body"));
        }

        // P2-6: verifier between passes, debug builds only. `note` first
        // (rigor escalates after a mandatory-lowering pass), then the
        // tree checks and one `check_table` per side table that exists.
        #[cfg(debug_assertions)]
        {
            verify.note(event);
            let folio = vize_s2::folio::S2Folio::of(&lowered.root.ops);
            verify.check(event, &folio);
            verify.check_table(event, &folio, &lowered.scopes);
            verify.check_table(event, &folio, &lowered.texts);
            verify.check_table(event, &folio, &lowered.wrappers);
            verify.check_table(event, &folio, &lowered.for_wrappers);
            verify.check_table(event, &folio, &facts.if_facts);
            verify.check_table(event, &folio, &facts.for_facts);
            verify.check_table(event, &folio, &facts.slot_facts);
            verify.check_table(event, &folio, &facts.text_facts);
            verify.check_table(event, &folio, &facts.model_faults);
            verify.check_table(event, &folio, &facts.static_facts);
        }
        Ok(())
    });
    // The catalogue above is closed over the const pipeline, so a failure
    // here is a compiler bug, not an input property.
    if let Err(failure) = outcome {
        panic!("s2 transform pipeline stopped: {}", failure.reason);
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::{
        S2_STAGE, TRANSFORM, TRANSFORM_LANE_FLAG, TRANSFORM_PASSES, hoist, text, vfor, vif, vmodel,
        vslot,
    };
    use vize_davinci::pass::{Fusability, PassKind, Preserved};

    #[test]
    fn the_pipeline_holds_exactly_the_landed_passes() {
        assert_eq!(TRANSFORM.stage, S2_STAGE);
        assert_eq!(TRANSFORM_PASSES.len(), 6);
        assert_eq!(TRANSFORM_PASSES[0], vif::DESC);
        assert_eq!(TRANSFORM_PASSES[1], vfor::DESC);
        assert_eq!(TRANSFORM_PASSES[2], vslot::DESC);
        assert_eq!(TRANSFORM_PASSES[3], text::DESC);
        assert_eq!(TRANSFORM_PASSES[4], vmodel::DESC);
        assert_eq!(TRANSFORM_PASSES[5], hoist::DESC);
    }

    #[test]
    fn the_vif_classification_is_pinned() {
        // The review-point classification, pinned so a drive-by re-kind
        // is a loud diff: see `vif::DESC`'s docs for the reasoning.
        assert_eq!(vif::DESC.name, "v-if");
        assert_eq!(vif::DESC.kind, PassKind::MandatoryLowering);
        assert_eq!(vif::DESC.fusability, Fusability::Barrier);
    }

    #[test]
    fn the_vfor_classification_is_pinned() {
        // The review-point classification, pinned so a drive-by re-kind
        // is a loud diff: see `vfor::DESC`'s docs for the reasoning
        // (a *preserving* mandatory pass — the recorded taxonomy
        // tension).
        assert_eq!(vfor::DESC.name, "v-for");
        assert_eq!(vfor::DESC.kind, PassKind::MandatoryLowering);
        assert_eq!(vfor::DESC.fusability, Fusability::Barrier);
    }

    #[test]
    fn the_vslot_classification_is_pinned() {
        // The review-point classification, pinned so a drive-by re-kind
        // is a loud diff: see `vslot::DESC`'s docs for the reasoning
        // (again a *preserving* mandatory pass — installment 2's
        // recorded taxonomy tension, in the milder diagnosing form).
        assert_eq!(vslot::DESC.name, "v-slot");
        assert_eq!(vslot::DESC.kind, PassKind::MandatoryLowering);
        assert_eq!(vslot::DESC.fusability, Fusability::Barrier);
    }

    #[test]
    fn the_text_classification_is_pinned() {
        // The review-point classification, pinned so a drive-by re-kind
        // is a loud diff: see `text::DESC`'s docs for the reasoning
        // (the vfor-shaped *preserving* mandatory pass — the recorded
        // taxonomy tension, third occurrence).
        assert_eq!(text::DESC.name, "text");
        assert_eq!(text::DESC.kind, PassKind::MandatoryLowering);
        assert_eq!(text::DESC.fusability, Fusability::Barrier);
    }

    #[test]
    fn the_vmodel_classification_is_pinned() {
        // The review-point classification, pinned so a drive-by re-kind
        // is a loud diff: see `vmodel::DESC`'s docs for the reasoning —
        // the series' FIRST `MandatoryDiagnostic` (the pass preserves
        // everything and its whole product is diagnostics plus the
        // fault record; nothing canonicalizes).
        assert_eq!(vmodel::DESC.name, "v-model");
        assert_eq!(vmodel::DESC.kind, PassKind::MandatoryDiagnostic);
        assert_eq!(vmodel::DESC.fusability, Fusability::Barrier);
    }

    #[test]
    fn the_hoist_classification_is_pinned() {
        // The review-point classification, pinned so a drive-by re-kind
        // is a loud diff: see `hoist::DESC`'s docs for the reasoning —
        // the series' FIRST `Optional` (skipping loses optimization
        // facts only; the shipped lane's own `hoist_static: false`
        // default is the proof) and FIRST `Fusable` (a synthesized-
        // attribute analysis, single-visit and local).
        assert_eq!(hoist::DESC.name, "hoist-static");
        assert_eq!(hoist::DESC.kind, PassKind::Optional);
        assert_eq!(hoist::DESC.fusability, Fusability::Fusable);
        assert_eq!(hoist::DESC.preserved, Preserved::ALL);
    }

    #[test]
    fn the_fusion_plan_is_five_lone_barriers_plus_one_fusable_singleton() {
        // The review point's fusion question, answered as data: the
        // five mandatory passes fuse with nothing (law 1), and the
        // series' first fusable pass lands in the first NON-barrier
        // group — a singleton, because its only neighbour is a barrier.
        // Fusion still buys nothing (six groups, six passes), but the
        // grouping machinery has now actually grouped a fusable pass.
        for index in 0..5 {
            let group = TRANSFORM.group(index).expect("group exists");
            assert!(group.is_barrier && group.len == 1);
        }
        let fusable = TRANSFORM.group(5).expect("the sixth group exists");
        assert!(!fusable.is_barrier);
        assert_eq!((fusable.start, fusable.len), (5, 1));
        assert!(fusable.preserved == Preserved::ALL);
        assert_eq!(TRANSFORM.group(6), None);
    }

    #[test]
    fn the_lane_flag_has_its_recorded_name() {
        assert_eq!(TRANSFORM_LANE_FLAG, "VIZE_DAVINCI_TRANSFORM");
    }
}
