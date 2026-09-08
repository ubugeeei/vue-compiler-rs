//! Lowering-published text facts.
//!
//! Whitespace condensing and text/interpolation merging already live in
//! [`crate::lower::text`], because both decisions need S1 comment
//! visibility. P2-12b removes the remaining transform walk too: a mixed
//! text run now validates and records its fact when the lowering mints
//! the compound op, then this module only mirrors that side table into
//! [`S2Facts`](super::S2Facts).
//!
//! The old `text` transform pass is deliberately not in the pipeline any
//! more. The invariant checks moved to the construction and legacy-filter
//! mutation sites, where the op span and rebuilt compound spelling are
//! still in hand.

use alloc::vec::Vec as StdVec;

use vize_davinci::side_table::SideTable;

use crate::lower::{Lowered, TextPart};

/// One compound op's consumed view: the merged run's parts, validated at
/// the lowering/mutation sites before DOM realization compiles from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFacts {
    /// The parts, in document order ([`TextPart`]'s field docs).
    pub parts: StdVec<TextPart>,
}

/// Facts cross compile boundaries with their artifact (P1-11).
const _: () = {
    const fn assert_owned<T: 'static>() {}
    assert_owned::<TextFacts>();
};

/// 64-bit footprint, guarded like every node-size assert (the wasm32
/// lane is 32-bit).
#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<TextFacts>() == 24);

/// Publish the lowering's already-validated compound text facts without
/// walking the op tree.
#[must_use]
pub(super) fn facts_from_lowering(lowered: &Lowered<'_>) -> SideTable<TextFacts> {
    let mut facts = SideTable::with_capacity(lowered.texts.len());
    for (id, parts) in lowered.texts.iter() {
        facts.insert(
            id,
            TextFacts {
                parts: parts.parts.clone(),
            },
        );
    }
    facts
}
