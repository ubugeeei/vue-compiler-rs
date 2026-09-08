//! Artifact-scoped S2 transform plans.
//!
//! The static transform table stays the review unit, but the build path
//! should not pay a diagnostic pass for an op family the lowering did not
//! produce. Feature bits come from the lowering construction sites, not a
//! second S2 walk.
//!
//! # Why the plan is a mask, not a match arm
//!
//! Each selectable pass is declined independently — its own op family is
//! absent, or the DOM emit declared it cannot consume the pass's product —
//! so the reachable plans are the *subsets* of [`SELECTABLE`], not a short
//! list of shapes. Enumerating them as named `const` pipelines costs one
//! item per combination and doubles with every pass that learns to
//! decline; at six selectable passes it is already 64 names for one
//! rule. [`plan_for_mask`] states the rule once instead, and [`PLANS`]
//! holds its answer for every mask so a [`Pipeline`]'s
//! `&'static [PassDesc]` still points at data nobody built at run time.
//!
//! The `const fn` is what keeps the compile-time pins available: the
//! `const _: () = assert!(…)` items below evaluate [`plan_for_mask`]
//! directly, so a plan-shape regression is a **compile error** exactly as
//! it was when each shape had a name (the P2-2 convention).
//!
//! # What a decline may cost
//!
//! Nothing the artifact can observe. A declined pass is one whose op
//! family the lowering never built, so its walk would publish an empty
//! fact table and raise no diagnostic — see each pass's `run` for the
//! `if let Op::…` that is its whole body, and
//! `tests/lowering_features.rs` for the behavioural pin. `hoist` is the
//! one decline made for a different reason: it is `Optional`, and the DOM
//! emit declines its product outright under `hoist_static: false`.

use vize_davinci::pass::{PassDesc, Pipeline};

use crate::lower::{LegacyCaps, LoweringFeatures};

use super::{S2_STAGE, hoist, legacy, vfor, vif, vmodel, vslot};

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

/// One selectable pass and the mask bit that keeps it.
///
/// Order here is the pipeline's execution order, so a plan is this table
/// filtered — never reordered. The passes touch disjoint op families and
/// carry no semantic dependency on one another (`super::TRANSFORM`'s
/// docs), which is what makes an arbitrary subset a legal pipeline.
const SELECTABLE: [(PassDesc, u8); 6] = [
    (legacy::DESC, LEGACY_SUGAR),
    (vif::DESC, IF_OPS),
    (vfor::DESC, FOR_OPS),
    (vslot::DESC, SLOT_CARRIERS),
    (vmodel::DESC, MODEL_BINDINGS),
    (hoist::DESC, STATIC_ANALYSIS),
];

const LEGACY_SUGAR: u8 = 1 << 0;
const IF_OPS: u8 = 1 << 1;
const FOR_OPS: u8 = 1 << 2;
const SLOT_CARRIERS: u8 = 1 << 3;
const MODEL_BINDINGS: u8 = 1 << 4;
const STATIC_ANALYSIS: u8 = 1 << 5;

/// Every mask, hence every plan.
const PLAN_COUNT: usize = 1 << SELECTABLE.len();

/// The passes one mask selects, in execution order.
///
/// `passes` is fixed-capacity because a [`Pipeline`] borrows
/// `&'static [PassDesc]`; slots past `len` are filler and never exposed —
/// [`Plan::pipeline`] is the only reader and it slices to `len`.
#[derive(Debug, Clone, Copy)]
struct Plan {
    passes: [PassDesc; SELECTABLE.len()],
    len: usize,
}

impl Plan {
    const EMPTY: Self = Self {
        passes: [hoist::DESC; SELECTABLE.len()],
        len: 0,
    };

    fn pipeline(&'static self) -> Pipeline {
        Pipeline::new(S2_STAGE, &self.passes[..self.len])
    }
}

/// The plan `mask` selects: [`SELECTABLE`] filtered in landed order.
const fn plan_for_mask(mask: u8) -> Plan {
    let mut plan = Plan::EMPTY;
    let mut index = 0;
    while index < SELECTABLE.len() {
        let (desc, bit) = SELECTABLE[index];
        if mask & bit != 0 {
            plan.passes[plan.len] = desc;
            plan.len += 1;
        }
        index += 1;
    }
    plan
}

const fn all_plans() -> [Plan; PLAN_COUNT] {
    let mut plans = [Plan::EMPTY; PLAN_COUNT];
    let mut mask = 0;
    while mask < PLAN_COUNT {
        // `mask` is bounded by `PLAN_COUNT`, which is `1 << 7`.
        #[expect(clippy::cast_possible_truncation)]
        let bits = mask as u8;
        plans[mask] = plan_for_mask(bits);
        mask += 1;
    }
    plans
}

/// Every plan, so a selected [`Pipeline`] borrows rather than builds.
static PLANS: [Plan; PLAN_COUNT] = all_plans();

// The full plan is the landed selectable table, and the empty plan is now
// truly empty: a plan-shape regression is a compile error, not a test failure.
const _: () = assert!(plan_for_mask(u8::MAX).len == 6);
const _: () = assert!(plan_for_mask(0).len == 0);
const _: () = assert!(plan_for_mask(STATIC_ANALYSIS).len == 1);

fn mask_for(caps: LegacyCaps, features: LoweringFeatures, profile: TransformProfile) -> u8 {
    let mut mask = 0;
    if caps.needs_sugar() {
        mask |= LEGACY_SUGAR;
    }
    if features.has_if_ops() {
        mask |= IF_OPS;
    }
    if features.has_for_ops() {
        mask |= FOR_OPS;
    }
    if features.has_slot_carriers() {
        mask |= SLOT_CARRIERS;
    }
    if features.has_model_bindings() {
        mask |= MODEL_BINDINGS;
    }
    if profile.includes_static_analysis() {
        mask |= STATIC_ANALYSIS;
    }
    mask
}

pub(super) fn pipeline_for_profile(
    caps: LegacyCaps,
    features: LoweringFeatures,
    profile: TransformProfile,
) -> Pipeline {
    PLANS[usize::from(mask_for(caps, features, profile))].pipeline()
}

#[cfg(test)]
mod tests;
