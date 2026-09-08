//! Lowering-published `v-for` facts.
//!
//! P2-9 originally kept a preserving mandatory `v-for` pass whose only
//! work was to rewalk S2, validate the lowering's scope table, and
//! publish the consumed `(value, key, index)` view. That validation now
//! happens at the lowering site that mints the `ui.for` scope; this
//! module is the compatibility mirror that presents the same fact table
//! to transform consumers without spending another S2 walk.
//!
//! The important law remains the same: every `ui.for` with an id has one
//! fresh scope tag, its named positions byte-equal the recorded
//! [`ScopeFacts`], and destructuring or escape-classified positions are
//! pessimistic [`ForName::Pending`] rather than synthesized names.
//!
//! [`ScopeFacts`]: vize_s2::scope::ScopeFacts

use vize_davinci::side_table::SideTable;

use crate::lower::Lowered;

pub use crate::lower::{ForName, ForParts as ForFacts};

/// The pass name kept for existing folio/report strings.
pub const NAME: &str = "v-for";

/// Facts cross compile boundaries with their artifact (P1-11; the same
/// enforcement `Diagnostic` and `ProvenanceRecord` carry).
const _: () = {
    const fn assert_owned<T: 'static>() {}
    assert_owned::<ForName>();
    assert_owned::<ForFacts>();
};

/// 64-bit footprints, guarded like every node-size assert (the wasm32
/// lane is 32-bit).
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(core::mem::size_of::<ForName>() == 24);
    assert!(core::mem::size_of::<ForFacts>() == 80);
};

/// Mirror lowering-published `ui.for` facts for transform consumers.
#[must_use]
pub fn facts_from_lowering(lowered: &Lowered<'_>) -> SideTable<ForFacts> {
    lowered.for_facts.clone()
}
