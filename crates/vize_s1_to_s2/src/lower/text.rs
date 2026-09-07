//! Whitespace condensing and text/interpolation merging — the P2-9
//! installment-4 port of the legacy text lane, absorbed into the
//! lowering.
//!
//! # Why this lives in the lowering, not a pass (the recorded decision)
//!
//! The legacy lane runs both halves *outside* its transform directory:
//! whitespace condensing at **parse time**
//! (`crates/vize_armature/src/parser/whitespace.rs`, driven by the
//! shipped `is_pre_tag`) and text/interpolation merging at **codegen
//! time** (`crates/vize_atelier_core/src/codegen/children.rs`, the
//! consecutive-run grouping); the old step file
//! (`crates/vize_atelier_core/src/steps/text.rs`) is exported but never
//! called by the shipped pipeline. The S2 port pulls both into the S1→S2 conversion,
//! for one decisive reason: **comments**. Both computations read comment
//! positions, and comments exist only in S1 unless the DOM
//! comment-preserving route asks the lowering to keep them as
//! `ui.comment` (the default lowering drops them under `drop.comment`).
//! Only the lowering still holds the information either computation
//! needs; a post-lowering pass would be comment-blind, and the shipped
//! answers are not.
//!
//! # What a comment does, per configuration
//!
//! **Preserved:** the comment is a real child — a non-text-like
//! neighbour for the remove-vs-condense rule and a hard boundary for run
//! merging.
//!
//! **Dropped (the shipped DOM default):** it is not a child at all.
//! Vue's parser builds no node for it, and its `onText` appends to the
//! previous child whenever that child is a text node, *without* a
//! contiguity check — so `a<!--c-->b` reaches whitespace condensing as
//! the single node `ab`, and `a<!--c-->\n<!--d-->b` as `a\nb`, which
//! condenses to `a b`. [`condense::plan_whitespace`](condense) therefore
//! absorbs a dropped comment into the text group around it and
//! [`run`](run) consumes it as a gap, so both halves see the child list
//! the shipped lane sees.
//!
//! An earlier reading had this the other way round — that a dropped
//! comment stays a run boundary, so `a<!--c-->b` "must stay two text
//! units". Measured against `@vue/compiler-dom` 3.5.41 and 3.6.0-beta.10
//! and against this workspace's own legacy lane, it does not: that
//! reading emitted `"a" + "b"` for `a<!--c-->b` and, worse,
//! `"a " + " b"` for `a <!--c--> b`, which renders one space too many.
//! `vize_atelier_dom/tests/dropped_comment_text_runs.rs` is the pin.
//! P2-5b's record
//! independently requires it: the position-classified opaque reasons —
//! [`OpaqueReason::Compound`] included — "are assignable only by the
//! S1→S2 lowering".
//!
//! # Condensing (Vue's `condense` strategy, the armature algorithm)
//!
//! Rule-for-rule the shipped `condense_whitespace`
//! (`whitespace.rs:69-165`), over the S1 child list of every lowered
//! region: leading/trailing whitespace-only text removed; a
//! whitespace-only run between two non-text-like neighbours with a
//! newline removed, condensed to one space otherwise; interior runs of
//! the Vue alphabet `[ \t\n\f\r]` in mixed text collapsed to one space.
//! Exemptions follow the shipped DOM configuration: `<pre>` subtrees
//! (`is_pre_tag`, `crates/vize_atelier_dom/src/compile/stage_options.rs`).
//!
//! # Merging, and the first `Compound` producer
//!
//! A maximal run of list-adjacent, span-contiguous text/interpolation
//! children lowers to **one op**: a text-only run to one `ui.text`
//! (concatenated content — natural census zero, the tokenizer emits
//! maximal runs; the lane exists for comment-punched and split trees),
//! and a mixed run to one `ui.interpolation` whose expression is
//! [`OpaqueReason::Compound`] — the escape class P2-5b reserved, gaining
//! its first producer here. The **pessimal laws apply from the first
//! byte** (`vize_s2::expr::opaque`): a compound is never constant,
//! equal to nothing, and emittable only byte-verbatim-or-refusal — so
//! the rebuilt [`OpaqueExpr::source`] is a *display* form, and DOM
//! realization (P2-11) compiles from the recorded parts instead
//! ([`TextParts`], validated and published by `pass::text`). The rebuild
//! spelling is canonical template syntax — static parts verbatim,
//! dynamic parts as `{{ <trimmed> }}` — shared verbatim with the pass's
//! re-derivation ([`rebuild_source`], the one-rule-two-sides
//! discipline). Merging never crosses a comment, a dropped node, or any
//! source gap: runs extend only over span-contiguous children, so the
//! merged span is the authored bytes exactly.
//!
//! [`OpaqueExpr::source`]: vize_s2::expr::OpaqueExpr::source

use alloc::vec::Vec as StdVec;

use vize_s0::{Span, String};

mod condense;
mod run;

pub(crate) use condense::{TextAction, plan_whitespace, suppresses_condense};
use condense::{collapse_fused, extends_run};
pub(crate) use run::{lower_text_run, lower_v_pre_text_run};

/// One part of a merged run, owned (the fact crosses compile boundaries
/// with its artifact, P1-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPart {
    /// The part's rendered text: condensed content for a static part,
    /// the trimmed expression for a dynamic one.
    pub text: String,
    /// The authored range (the raw token for a static part, the whole
    /// `{{ … }}` for a dynamic one).
    pub span: Span,
    /// Whether the part renders an expression.
    pub dynamic: bool,
}

/// The recorded parts of one merged mixed run, keyed by the compound
/// `ui.interpolation` op's id ([`crate::lower::Lowered::texts`]). The
/// `pass::text` consumption validates every entry against the op it
/// keys and publishes the consumed view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextParts {
    /// The parts, in document order.
    pub parts: StdVec<TextPart>,
}

const _: () = {
    const fn assert_owned<T: 'static>() {}
    assert_owned::<TextPart>();
    assert_owned::<TextParts>();
};

/// 64-bit footprints, guarded like every node-size assert (the wasm32
/// lane is 32-bit).
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(core::mem::size_of::<TextPart>() == 40);
    assert!(core::mem::size_of::<TextParts>() == 24);
};

/// The rebuild rule: the canonical template spelling of a merged run —
/// static parts verbatim, dynamic parts as `{{ <text> }}`. One home,
/// used by the lowering to mint [`OpaqueExpr::source`] and by the pass
/// to re-derive it (`pass::text`), so the two can only disagree when one
/// of them changed.
///
/// [`OpaqueExpr::source`]: vize_s2::expr::OpaqueExpr::source
pub fn rebuild_source(parts: &[TextPart]) -> String {
    let mut out = String::default();
    for part in parts {
        if part.dynamic {
            out.push_str("{{ ");
            out.push_str(part.text.as_str());
            out.push_str(" }}");
        } else {
            out.push_str(part.text.as_str());
        }
    }
    out
}

pub(crate) fn legacy_slot_filler_text(text: &str) -> bool {
    if text.trim().is_empty() {
        return true;
    }
    if !text.contains('&') {
        return false;
    }

    let mut index = 0usize;
    while index < text.len() {
        let tail = &text[index..];
        let ch = tail.chars().next().expect("index is in-bounds");
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }
        if let Some(consumed) = nbsp_entity_len(tail) {
            index += consumed;
            continue;
        }
        return false;
    }
    true
}

pub(crate) fn legacy_slot_filler_needs_props_placeholder(text: &str) -> bool {
    legacy_slot_filler_text(text)
        && text
            .chars()
            .any(|ch| ch == '&' || !ch.is_ascii_whitespace())
}

fn nbsp_entity_len(text: &str) -> Option<usize> {
    if text.starts_with("&nbsp;") {
        return Some("&nbsp;".len());
    }
    if let Some(rest) = text.strip_prefix("&nbsp")
        && (rest.is_empty()
            || rest.starts_with('&')
            || rest.chars().next().is_some_and(char::is_whitespace))
    {
        return Some("&nbsp".len());
    }
    numeric_nbsp_entity_len(text)
}

fn numeric_nbsp_entity_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if !bytes.starts_with(b"&#") {
        return None;
    }

    let mut index = 2usize;
    let radix = if matches!(bytes.get(index), Some(b'x' | b'X')) {
        index += 1;
        16
    } else {
        10
    };
    let start = index;
    while let Some(byte) = bytes.get(index) {
        let digit = if radix == 16 {
            byte.is_ascii_hexdigit()
        } else {
            byte.is_ascii_digit()
        };
        if !digit {
            break;
        }
        index += 1;
    }
    if index == start {
        return None;
    }
    let digits = core::str::from_utf8(&bytes[start..index]).ok()?;
    if u32::from_str_radix(digits, radix).ok()? != 0xa0 {
        return None;
    }
    if bytes.get(index) == Some(&b';') {
        index += 1;
    }
    Some(index)
}
