//! Lowering-published `v-if` branch-key facts.
//!
//! P2-9 originally kept a mandatory `v-if` transform pass whose only
//! remaining work was to lift branch-carrier `key` props into
//! [`IfFacts`] and diagnose duplicate keys. The lowering now performs
//! that work where the chain, branch order, wrapper captures, and
//! carrier surface are already available. This module is the
//! compatibility mirror that keeps existing pass consumers on the same
//! public names without spending another S2 walk.

use vize_davinci::side_table::SideTable;

use crate::lower::Lowered;

pub use crate::lower::SAME_KEY_MESSAGE;
pub use crate::lower::{BranchKey, BranchKeyKind, IfFacts};

/// The pass name kept for existing folio/report strings.
pub const NAME: &str = "v-if";

/// Mirror lowering-published `ui.if` facts for transform consumers.
#[must_use]
pub fn facts_from_lowering(lowered: &Lowered<'_>) -> SideTable<IfFacts> {
    lowered.if_facts.clone()
}
