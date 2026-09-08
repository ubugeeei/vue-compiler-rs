//! The lowering's observed feature bits.
//!
//! # Why the bits exist
//!
//! Most remaining mandatory S2 passes are the consumer of one op family:
//! [`pass::vif`](crate::pass::vif) reads `ui.if`,
//! [`pass::vslot`](crate::pass::vslot) reads the slot carriers, and
//! [`pass::vmodel`](crate::pass::vmodel) reads `ui.model`. Run against an
//! artifact whose family the lowering never built, such a pass walks the
//! whole tree to publish an empty fact table and raise no diagnostic — a
//! walk the build path can decline without declining any product.
//! Compound text and `ui.for` remain observed here, but P2-12b publishes
//! their facts at lowering time, so neither selects a transform pass.
//!
//! # Why the bit is set at the op, not read off provenance
//!
//! The first cut derived these bits from the lowering's provenance rule
//! names (`lower.for`, `lower.if`, …), which is one scan of a stream the
//! lowering already wrote and costs no traversal. It is also **wrong**,
//! and a committed test says so: `<p v-for="items">` cannot split under
//! Vue's grammar, so `lower/forop.rs` takes the escape arm, records
//! `error.v-for-malformed` instead of `lower.for` — and still leaves a
//! `ui.for` op in the tree. Provenance records *decisions*; the planner
//! is asking about *ops*, and a failed decision is exactly the case where
//! the two diverge.
//!
//! So [`Cx::observe`](super::cx::Cx::observe) sets the bit where the op is
//! minted. There is no second scan and no name to keep in sync: the type
//! system asks for an [`OpFamily`] at every construction site.
//!
//! # The one assumption left
//!
//! A family that gains a *new* construction site needs `observe` there
//! too. `crates/vize_s1_to_s2/tests/lowering_features.rs` is the pin: it
//! lowers a template per family — malformed spellings included — and
//! fails if the bit is missing, then proves the planned run and the full
//! four-pass table agree on every product an artifact has.

/// An op family whose presence can select a pass or publish facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpFamily {
    /// A `ui.if`, however malformed its branches.
    If,
    /// A `ui.for`, the undecomposable escape included.
    For,
    /// Anything [`pass::vslot`](crate::pass::vslot) can produce a fact or
    /// a diagnostic at: a `ui.component` (the canonical grouping, the
    /// implicit-default synthesis and three diagnostics), a `ui.slot`
    /// outlet or a `ui.slot-content` binding (the two `VSlotMisplaced`
    /// anchors, which fire with no component in sight).
    SlotCarrier,
    /// A compound text/interpolation run whose structured parts were
    /// validated and attached by lowering.
    TextCompound,
    /// A `ui.model` binding.
    Model,
}

/// Feature bits the lowering observed while building S2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoweringFeatures {
    bits: u8,
}

impl LoweringFeatures {
    /// No family built: the artifact every mandatory pass may decline.
    pub const EMPTY: Self = Self { bits: 0 };

    /// Whether the lowering built any `ui.if`.
    #[must_use]
    pub const fn has_if_ops(self) -> bool {
        self.holds(OpFamily::If)
    }

    /// Whether the lowering built any `ui.for`.
    #[must_use]
    pub const fn has_for_ops(self) -> bool {
        self.holds(OpFamily::For)
    }

    /// Whether the lowering built any slot carrier.
    #[must_use]
    pub const fn has_slot_carriers(self) -> bool {
        self.holds(OpFamily::SlotCarrier)
    }

    /// Whether the lowering built any compound text/interpolation run.
    #[must_use]
    pub const fn has_text_compounds(self) -> bool {
        self.holds(OpFamily::TextCompound)
    }

    /// Whether the lowering built any `ui.model` binding.
    #[must_use]
    pub const fn has_model_bindings(self) -> bool {
        self.holds(OpFamily::Model)
    }

    /// The same bits with `family` observed.
    #[must_use]
    pub const fn observing(self, family: OpFamily) -> Self {
        Self {
            bits: self.bits | family.bit(),
        }
    }

    const fn holds(self, family: OpFamily) -> bool {
        self.bits & family.bit() != 0
    }
}

impl OpFamily {
    const fn bit(self) -> u8 {
        match self {
            OpFamily::If => 1 << 0,
            OpFamily::For => 1 << 1,
            OpFamily::SlotCarrier => 1 << 2,
            OpFamily::TextCompound => 1 << 3,
            OpFamily::Model => 1 << 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LoweringFeatures, OpFamily};

    const EVERY: [OpFamily; 5] = [
        OpFamily::If,
        OpFamily::For,
        OpFamily::SlotCarrier,
        OpFamily::TextCompound,
        OpFamily::Model,
    ];

    #[test]
    fn empty_holds_no_family() {
        let features = LoweringFeatures::EMPTY;
        assert!(!features.has_model_bindings());
        assert!(!features.has_if_ops());
        assert!(!features.has_for_ops());
        assert!(!features.has_slot_carriers());
        assert!(!features.has_text_compounds());
        assert_eq!(features, LoweringFeatures::default());
    }

    /// Every family owns a distinct bit, so observing one can never
    /// answer for another — the property the planner's declines rest on.
    #[test]
    fn each_family_owns_one_distinct_bit() {
        let mut seen = 0u8;
        for family in EVERY {
            let bit = family.bit();
            assert_eq!(bit.count_ones(), 1, "{family:?} must name one bit");
            assert_eq!(seen & bit, 0, "{family:?} shares a bit with an earlier one");
            seen |= bit;
        }
    }

    #[test]
    fn observing_is_additive_and_idempotent() {
        let once = LoweringFeatures::EMPTY.observing(OpFamily::For);
        assert_eq!(once.observing(OpFamily::For), once);
        assert!(once.has_for_ops());
        assert!(!once.has_if_ops());
        assert!(!once.has_slot_carriers());
        assert!(!once.has_text_compounds());
        assert!(!once.has_model_bindings());

        let all = EVERY
            .into_iter()
            .fold(LoweringFeatures::EMPTY, LoweringFeatures::observing);
        assert!(all.has_if_ops() && all.has_for_ops());
        assert!(all.has_slot_carriers() && all.has_model_bindings());
    }
}
