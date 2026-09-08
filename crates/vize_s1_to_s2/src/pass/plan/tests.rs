//! The plan rule's suite: the static table against the rule, every
//! plan against the landed pipeline's order, and one case per decline.

use super::{
    FOR_OPS, IF_OPS, LEGACY_SUGAR, MODEL_BINDINGS, PLAN_COUNT, PLANS, SELECTABLE, SLOT_CARRIERS,
    STATIC_ANALYSIS, mask_for, pipeline_for_profile, plan_for_mask,
};
use crate::lower::{LegacyCaps, LoweringFeatures, OpFamily};
use crate::pass::{TRANSFORM, TRANSFORM_PASSES, TransformProfile, hoist, legacy, vmodel};
use vize_s0::config::VueVersion;

const EVERY: [OpFamily; 5] = [
    OpFamily::If,
    OpFamily::For,
    OpFamily::SlotCarrier,
    OpFamily::TextCompound,
    OpFamily::Model,
];

fn every_family() -> LoweringFeatures {
    EVERY
        .into_iter()
        .fold(LoweringFeatures::EMPTY, LoweringFeatures::observing)
}

fn only(family: OpFamily) -> LoweringFeatures {
    LoweringFeatures::EMPTY.observing(family)
}

fn default_pipeline_for(
    caps: LegacyCaps,
    features: LoweringFeatures,
) -> vize_davinci::pass::Pipeline {
    pipeline_for_profile(caps, features, TransformProfile::DEFAULT)
}

/// Assert a pipeline's pass names, in order. Spelled as a zip rather
/// than a collected `Vec` so this suite owns no storage — the file is a
/// separate module, so `davinci-storage-policy`'s `cfg(test)` masking
/// does not reach it.
#[track_caller]
fn assert_names(pipeline: &vize_davinci::pass::Pipeline, expected: &[&str]) {
    assert_eq!(
        pipeline.passes.len(),
        expected.len(),
        "pass count: {:?} vs {expected:?}",
        pipeline.passes.iter().map(|pass| pass.name),
    );
    for (index, (pass, name)) in pipeline.passes.iter().zip(expected).enumerate() {
        assert_eq!(pass.name, *name, "pass {index}");
    }
}

#[test]
fn the_static_table_agrees_with_the_rule_for_every_mask() {
    for (mask, plan) in PLANS.iter().enumerate() {
        let computed = plan_for_mask(u8::try_from(mask).expect("PLAN_COUNT fits u8"));
        assert_eq!(plan.len, computed.len, "mask {mask} length");
        assert_eq!(
            plan.passes[..plan.len],
            computed.passes[..computed.len],
            "mask {mask} passes"
        );
    }
}

/// Every plan is a subsequence of the landed table, never a
/// reordering of it: declining a pass may not move another one.
#[test]
fn every_plan_is_a_subsequence_of_the_full_pipeline() {
    for (mask, plan) in PLANS.iter().enumerate() {
        let selected = &plan.passes[..plan.len];
        // The Vue 2 sugar pass leads or is absent; it is not part of the
        // Vue 3 table, so the order check below skips it.
        assert!(
            selected
                .iter()
                .skip(1)
                .all(|pass| pass.name != legacy::DESC.name),
            "mask {mask}: sugar may only lead",
        );
        let mut next = 0;
        for pass in selected
            .iter()
            .filter(|pass| pass.name != legacy::DESC.name)
        {
            let found = TRANSFORM_PASSES[next..]
                .iter()
                .position(|full| full.name == pass.name)
                .unwrap_or_else(|| panic!("mask {mask}: {} out of pipeline order", pass.name));
            next += found + 1;
        }
    }
}

/// One walk per pass today, so a plan's group count is its length —
/// the honest reading of "declining a pass saves a walk".
#[test]
fn a_plans_walk_count_is_its_pass_count() {
    for (mask, plan) in PLANS.iter().enumerate() {
        let pipeline = plan.pipeline();
        assert_eq!(pipeline.passes.len(), plan.len, "mask {mask} length");
        assert_eq!(pipeline.group_count(), plan.len, "mask {mask} groups");
    }
}

#[test]
fn vue3_with_every_family_is_the_landed_five_pass_table() {
    let pipeline = default_pipeline_for(LegacyCaps::VUE3, every_family());
    assert_eq!(pipeline, TRANSFORM);
    assert_eq!(pipeline.passes.len(), 5);
    assert_eq!(pipeline.group_count(), 5);
}

#[test]
fn vue3_omits_the_model_pass_when_lowering_found_no_model_ops() {
    let features = every_family();
    let pipeline = default_pipeline_for(
        LegacyCaps::VUE3,
        only(OpFamily::If)
            .observing(OpFamily::For)
            .observing(OpFamily::SlotCarrier),
    );
    assert!(features.has_model_bindings());
    assert_names(&pipeline, &["v-if", "v-for", "v-slot", hoist::NAME]);
    assert!(!pipeline.passes.iter().any(|pass| pass.name == vmodel::NAME));
}

#[test]
fn vue3_keeps_the_model_pass_when_diagnostics_may_need_it() {
    let pipeline = default_pipeline_for(LegacyCaps::VUE3, every_family());
    assert_eq!(pipeline.passes[3], vmodel::DESC);
}

/// The headline of this installment: an artifact with none of the
/// three structural families pays neither their walks nor the model
/// pass's; if it also has no compound text, the optional analysis is the
/// only transform walk left.
#[test]
fn vue3_omits_every_structural_pass_when_no_family_was_lowered() {
    let pipeline = default_pipeline_for(LegacyCaps::VUE3, LoweringFeatures::EMPTY);
    assert_names(&pipeline, &[hoist::NAME]);
    assert_eq!(pipeline.group_count(), 1);
}

#[test]
fn each_structural_family_buys_back_exactly_its_own_pass() {
    let cases = [
        (only(OpFamily::If), "v-if"),
        (only(OpFamily::For), "v-for"),
        (only(OpFamily::SlotCarrier), "v-slot"),
    ];
    for (features, expected) in cases {
        let pipeline = default_pipeline_for(LegacyCaps::VUE3, features);
        assert_names(&pipeline, &[expected, hoist::NAME]);
    }
}

#[test]
fn vue3_does_not_plan_a_text_walk_when_compounds_need_facts() {
    let pipeline = default_pipeline_for(LegacyCaps::VUE3, only(OpFamily::TextCompound));
    assert_names(&pipeline, &[hoist::NAME]);
}

#[test]
fn vue3_omits_static_analysis_when_dom_emit_cannot_use_it() {
    let profile = TransformProfile::DEFAULT.without_static_analysis();
    let pipeline = pipeline_for_profile(LegacyCaps::VUE3, every_family(), profile);
    assert_eq!(pipeline.passes.len(), 4);
    assert!(!pipeline.passes.iter().any(|pass| pass.name == hoist::NAME));
    assert_eq!(pipeline.passes[3], vmodel::DESC);

    let bare = pipeline_for_profile(LegacyCaps::VUE3, LoweringFeatures::EMPTY, profile);
    assert_names(&bare, &[]);
    assert_eq!(bare.group_count(), 0);
}

#[test]
fn vue2_legacy_sugar_still_prepends_the_selected_vue3_shape() {
    let caps = LegacyCaps::for_version(VueVersion::V2);
    let full = default_pipeline_for(caps, every_family());
    assert_eq!(full.passes.len(), 6);
    assert_eq!(full.group_count(), 6);
    assert_eq!(full.passes[0], legacy::DESC);

    let bare = default_pipeline_for(caps, LoweringFeatures::EMPTY);
    assert_names(&bare, &[legacy::DESC.name, hoist::NAME]);
}

#[test]
fn vue2_legacy_sugar_preserves_the_dom_emit_static_analysis_choice() {
    let caps = LegacyCaps::for_version(VueVersion::V2);
    let profile = TransformProfile::DEFAULT.without_static_analysis();

    let full = pipeline_for_profile(caps, every_family(), profile);
    assert_eq!(full.passes.len(), 5);
    assert!(!full.passes.iter().any(|pass| pass.name == hoist::NAME));

    let bare = pipeline_for_profile(caps, LoweringFeatures::EMPTY, profile);
    assert_names(&bare, &[legacy::DESC.name]);
}

#[test]
fn the_mask_reads_one_bit_per_selectable_pass() {
    assert_eq!(SELECTABLE.len(), 6);
    assert_eq!(PLAN_COUNT, 64);
    let bits = [
        LEGACY_SUGAR,
        IF_OPS,
        FOR_OPS,
        SLOT_CARRIERS,
        MODEL_BINDINGS,
        STATIC_ANALYSIS,
    ];
    for (index, (_, bit)) in SELECTABLE.iter().enumerate() {
        assert_eq!(*bit, bits[index], "SELECTABLE order must match the bits");
    }
    assert_eq!(
        mask_for(
            LegacyCaps::for_version(VueVersion::V2),
            every_family(),
            TransformProfile::DEFAULT
        ),
        u8::try_from(PLAN_COUNT - 1).expect("six bits"),
    );
    assert_eq!(
        mask_for(
            LegacyCaps::VUE3,
            LoweringFeatures::EMPTY,
            TransformProfile::DEFAULT.without_static_analysis()
        ),
        0
    );
}
