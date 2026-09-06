//! Artifact-scoped S2 transform plans.
//!
//! The static six-pass table stays the review unit, but the build path
//! should not pay a diagnostic pass for an op family the lowering did not
//! produce. Feature bits come from lowering provenance, not a second S2 walk.

use vize_davinci::pass::{PassDesc, Pipeline};

use crate::lower::{LegacyCaps, LoweringFeatures};

use super::{S2_STAGE, TRANSFORM, hoist, legacy, text, vfor, vif, vslot};

pub(super) const TRANSFORM_WITHOUT_MODEL_PASSES: &[PassDesc] =
    &[vif::DESC, vfor::DESC, vslot::DESC, text::DESC, hoist::DESC];

pub(super) const TRANSFORM_WITHOUT_MODEL: Pipeline =
    Pipeline::new(S2_STAGE, TRANSFORM_WITHOUT_MODEL_PASSES);

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

const _: () = assert!(TRANSFORM_WITHOUT_MODEL.group_count() == 5);
const _: () = assert!(LEGACY_WITHOUT_MODEL.group_count() == 6);

pub(super) const fn pipeline_for(caps: LegacyCaps, features: LoweringFeatures) -> Pipeline {
    match (caps.needs_sugar(), features.has_model_bindings()) {
        (true, true) => legacy::LEGACY,
        (true, false) => LEGACY_WITHOUT_MODEL,
        (false, true) => TRANSFORM,
        (false, false) => TRANSFORM_WITHOUT_MODEL,
    }
}

#[cfg(test)]
mod tests {
    use super::{LEGACY_WITHOUT_MODEL, TRANSFORM_WITHOUT_MODEL, pipeline_for};
    use crate::lower::{LegacyCaps, LoweringFeatures};
    use crate::pass::{TRANSFORM, hoist, legacy, vmodel};
    use vize_s0::config::VueVersion;

    fn with_model() -> LoweringFeatures {
        LoweringFeatures::EMPTY.with_model_bindings()
    }

    #[test]
    fn vue3_omits_the_model_pass_when_lowering_found_no_model_ops() {
        let pipeline = pipeline_for(LegacyCaps::VUE3, LoweringFeatures::EMPTY);
        assert_eq!(pipeline, TRANSFORM_WITHOUT_MODEL);
        assert_eq!(pipeline.passes.len(), 5);
        assert_eq!(pipeline.group_count(), 5);
        assert_eq!(pipeline.passes[4], hoist::DESC);
        assert!(!pipeline.passes.iter().any(|pass| pass.name == vmodel::NAME));
    }

    #[test]
    fn vue3_keeps_the_model_pass_when_diagnostics_may_need_it() {
        let pipeline = pipeline_for(LegacyCaps::VUE3, with_model());
        assert_eq!(pipeline, TRANSFORM);
        assert_eq!(pipeline.passes.len(), 6);
        assert_eq!(pipeline.group_count(), 6);
        assert_eq!(pipeline.passes[4], vmodel::DESC);
    }

    #[test]
    fn vue2_legacy_sugar_still_prepends_the_selected_vue3_shape() {
        let caps = LegacyCaps::for_version(VueVersion::V2);
        let without_model = pipeline_for(caps, LoweringFeatures::EMPTY);
        assert_eq!(without_model, LEGACY_WITHOUT_MODEL);
        assert_eq!(without_model.passes.len(), 6);
        assert_eq!(without_model.group_count(), 6);

        let with_model = pipeline_for(caps, with_model());
        assert_eq!(with_model, legacy::LEGACY);
        assert_eq!(with_model.passes.len(), 7);
        assert_eq!(with_model.group_count(), 7);
    }
}
