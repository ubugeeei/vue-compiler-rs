//! Artifact-scoped S2 transform plans.
//!
//! The static six-pass table stays the review unit, but the build path
//! should not pay a diagnostic pass for an op family the lowering did not
//! produce. Feature bits come from lowering provenance, not a second S2 walk.

use vize_davinci::pass::{PassDesc, Pipeline};

use crate::lower::{LegacyCaps, LoweringFeatures};

use super::{S2_STAGE, TRANSFORM, hoist, legacy, text, vfor, vif, vmodel, vslot};

/// Product-facing selection for optional S2 transform work.
///
/// The transform catalogue stays the full review surface. One-shot DOM
/// emission can still decline facts it cannot consume: with
/// `hoist_static: false`, `hoist-static` would only spend an S2 walk to
/// produce facts that the emitter is required to ignore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransformProfile {
    include_static_analysis: bool,
}

impl TransformProfile {
    /// The full transform plan.
    pub const DEFAULT: Self = Self {
        include_static_analysis: true,
    };

    /// Drop the optional `hoist-static` analysis pass.
    #[must_use]
    pub const fn without_static_analysis(self) -> Self {
        Self {
            include_static_analysis: false,
        }
    }

    #[must_use]
    pub const fn includes_static_analysis(self) -> bool {
        self.include_static_analysis
    }
}

pub(super) const TRANSFORM_WITHOUT_MODEL_PASSES: &[PassDesc] =
    &[vif::DESC, vfor::DESC, vslot::DESC, text::DESC, hoist::DESC];

pub(super) const TRANSFORM_WITHOUT_MODEL: Pipeline =
    Pipeline::new(S2_STAGE, TRANSFORM_WITHOUT_MODEL_PASSES);

pub(super) const TRANSFORM_WITHOUT_HOIST_PASSES: &[PassDesc] =
    &[vif::DESC, vfor::DESC, vslot::DESC, text::DESC, vmodel::DESC];

pub(super) const TRANSFORM_WITHOUT_HOIST: Pipeline =
    Pipeline::new(S2_STAGE, TRANSFORM_WITHOUT_HOIST_PASSES);

pub(super) const TRANSFORM_WITHOUT_MODEL_OR_HOIST_PASSES: &[PassDesc] =
    &[vif::DESC, vfor::DESC, vslot::DESC, text::DESC];

pub(super) const TRANSFORM_WITHOUT_MODEL_OR_HOIST: Pipeline =
    Pipeline::new(S2_STAGE, TRANSFORM_WITHOUT_MODEL_OR_HOIST_PASSES);

pub(super) const LEGACY_WITHOUT_MODEL_PASSES: &[PassDesc] = &[
    legacy::DESC,
    vif::DESC,
    vfor::DESC,
    vslot::DESC,
    text::DESC,
    hoist::DESC,
];

pub(super) const LEGACY_WITHOUT_MODEL: Pipeline =
    Pipeline::new(S2_STAGE, LEGACY_WITHOUT_MODEL_PASSES);

pub(super) const LEGACY_WITHOUT_HOIST_PASSES: &[PassDesc] = &[
    legacy::DESC,
    vif::DESC,
    vfor::DESC,
    vslot::DESC,
    text::DESC,
    vmodel::DESC,
];

pub(super) const LEGACY_WITHOUT_HOIST: Pipeline =
    Pipeline::new(S2_STAGE, LEGACY_WITHOUT_HOIST_PASSES);

pub(super) const LEGACY_WITHOUT_MODEL_OR_HOIST_PASSES: &[PassDesc] =
    &[legacy::DESC, vif::DESC, vfor::DESC, vslot::DESC, text::DESC];

pub(super) const LEGACY_WITHOUT_MODEL_OR_HOIST: Pipeline =
    Pipeline::new(S2_STAGE, LEGACY_WITHOUT_MODEL_OR_HOIST_PASSES);

const _: () = assert!(TRANSFORM_WITHOUT_MODEL.group_count() == 5);
const _: () = assert!(TRANSFORM_WITHOUT_HOIST.group_count() == 5);
const _: () = assert!(TRANSFORM_WITHOUT_MODEL_OR_HOIST.group_count() == 4);
const _: () = assert!(LEGACY_WITHOUT_MODEL.group_count() == 6);
const _: () = assert!(LEGACY_WITHOUT_HOIST.group_count() == 6);
const _: () = assert!(LEGACY_WITHOUT_MODEL_OR_HOIST.group_count() == 5);

pub(super) const fn pipeline_for_profile(
    caps: LegacyCaps,
    features: LoweringFeatures,
    profile: TransformProfile,
) -> Pipeline {
    match (
        caps.needs_sugar(),
        features.has_model_bindings(),
        profile.includes_static_analysis(),
    ) {
        (true, true, true) => legacy::LEGACY,
        (true, true, false) => LEGACY_WITHOUT_HOIST,
        (true, false, true) => LEGACY_WITHOUT_MODEL,
        (true, false, false) => LEGACY_WITHOUT_MODEL_OR_HOIST,
        (false, true, true) => TRANSFORM,
        (false, true, false) => TRANSFORM_WITHOUT_HOIST,
        (false, false, true) => TRANSFORM_WITHOUT_MODEL,
        (false, false, false) => TRANSFORM_WITHOUT_MODEL_OR_HOIST,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LEGACY_WITHOUT_HOIST, LEGACY_WITHOUT_MODEL, LEGACY_WITHOUT_MODEL_OR_HOIST,
        TRANSFORM_WITHOUT_HOIST, TRANSFORM_WITHOUT_MODEL, TRANSFORM_WITHOUT_MODEL_OR_HOIST,
        pipeline_for_profile,
    };
    use crate::lower::{LegacyCaps, LoweringFeatures};
    use crate::pass::{TRANSFORM, TransformProfile, hoist, legacy, vmodel};
    use vize_s0::config::VueVersion;

    fn with_model() -> LoweringFeatures {
        LoweringFeatures::EMPTY.with_model_bindings()
    }

    fn default_pipeline_for(
        caps: LegacyCaps,
        features: LoweringFeatures,
    ) -> vize_davinci::pass::Pipeline {
        pipeline_for_profile(caps, features, TransformProfile::DEFAULT)
    }

    #[test]
    fn vue3_omits_the_model_pass_when_lowering_found_no_model_ops() {
        let pipeline = default_pipeline_for(LegacyCaps::VUE3, LoweringFeatures::EMPTY);
        assert_eq!(pipeline, TRANSFORM_WITHOUT_MODEL);
        assert_eq!(pipeline.passes.len(), 5);
        assert_eq!(pipeline.group_count(), 5);
        assert_eq!(pipeline.passes[4], hoist::DESC);
        assert!(!pipeline.passes.iter().any(|pass| pass.name == vmodel::NAME));
    }

    #[test]
    fn vue3_keeps_the_model_pass_when_diagnostics_may_need_it() {
        let pipeline = default_pipeline_for(LegacyCaps::VUE3, with_model());
        assert_eq!(pipeline, TRANSFORM);
        assert_eq!(pipeline.passes.len(), 6);
        assert_eq!(pipeline.group_count(), 6);
        assert_eq!(pipeline.passes[4], vmodel::DESC);
    }

    #[test]
    fn vue3_omits_static_analysis_when_dom_emit_cannot_use_it() {
        let profile = TransformProfile::DEFAULT.without_static_analysis();

        let without_model =
            pipeline_for_profile(LegacyCaps::VUE3, LoweringFeatures::EMPTY, profile);
        assert_eq!(without_model, TRANSFORM_WITHOUT_MODEL_OR_HOIST);
        assert_eq!(without_model.passes.len(), 4);
        assert_eq!(without_model.group_count(), 4);
        assert!(
            !without_model
                .passes
                .iter()
                .any(|pass| pass.name == hoist::NAME)
        );
        assert!(
            !without_model
                .passes
                .iter()
                .any(|pass| pass.name == vmodel::NAME)
        );

        let with_model = pipeline_for_profile(LegacyCaps::VUE3, with_model(), profile);
        assert_eq!(with_model, TRANSFORM_WITHOUT_HOIST);
        assert_eq!(with_model.passes.len(), 5);
        assert_eq!(with_model.group_count(), 5);
        assert_eq!(with_model.passes[4], vmodel::DESC);
        assert!(
            !with_model
                .passes
                .iter()
                .any(|pass| pass.name == hoist::NAME)
        );
    }

    #[test]
    fn vue2_legacy_sugar_still_prepends_the_selected_vue3_shape() {
        let caps = LegacyCaps::for_version(VueVersion::V2);
        let without_model = default_pipeline_for(caps, LoweringFeatures::EMPTY);
        assert_eq!(without_model, LEGACY_WITHOUT_MODEL);
        assert_eq!(without_model.passes.len(), 6);
        assert_eq!(without_model.group_count(), 6);

        let with_model = default_pipeline_for(caps, with_model());
        assert_eq!(with_model, legacy::LEGACY);
        assert_eq!(with_model.passes.len(), 7);
        assert_eq!(with_model.group_count(), 7);
    }

    #[test]
    fn vue2_legacy_sugar_preserves_the_dom_emit_static_analysis_choice() {
        let caps = LegacyCaps::for_version(VueVersion::V2);
        let profile = TransformProfile::DEFAULT.without_static_analysis();

        let without_model = pipeline_for_profile(caps, LoweringFeatures::EMPTY, profile);
        assert_eq!(without_model, LEGACY_WITHOUT_MODEL_OR_HOIST);
        assert_eq!(without_model.passes.len(), 5);
        assert_eq!(without_model.group_count(), 5);

        let with_model = pipeline_for_profile(caps, with_model(), profile);
        assert_eq!(with_model, LEGACY_WITHOUT_HOIST);
        assert_eq!(with_model.passes.len(), 6);
        assert_eq!(with_model.group_count(), 6);
    }
}
