//! The lowering entry: S1 surface tree in, S2 artifact out — **total,
//! no rollback** (the MLIR import).
//!
//! [`lower`] never panics and never abandons partial work: every S1
//! construct either becomes an op of the existing S2 family or leaves a
//! [`Diagnostic`] beside the kept fragments, and the three fact channels
//! (diagnostics, provenance, hygiene scopes) are all populated by the
//! time the function returns — including on inputs that are wrong from
//! the first byte. The tokenizer's [`SurfaceError`]s enter the unified
//! channel here (`Stage::Surface`, the exact `ErrorCode` message),
//! ahead of the lowering's own `Stage::Semantic` findings.
//!
//! # Id accounting (the numbering law)
//!
//! Ops are numbered densely in page order as they are decided
//! ([`lower::cx`]); [`Lowered::op_count`] equals
//! `S2Folio::of(&lowered.root.ops).op_count()` on every input —
//! the law the side tables and the S2 verifier's `verify_table` key on.
//!
//! [`Diagnostic`]: vize_davinci::diagnostic::Diagnostic
//! [`S2Folio`]: vize_s2::folio::S2Folio
//! [`SurfaceError`]: vize_s1::SurfaceError

use alloc::vec::Vec as StdVec;
use core::fmt;

use vize_davinci::diagnostic::{Diagnostic, Severity, Stage};
use vize_davinci::side_table::SideTable;
use vize_s0::{Allocator, SourceBlock, SourceRoot, Span, String};
use vize_s1::{SurfaceError, SurfaceTree};

use vize_s2::op::{Namespace, Region};
use vize_s2::provenance::ProvenanceRecord;
use vize_s2::scope::ScopeFacts;

mod binding;
mod bindop;
mod caps;
mod cloak;
mod css;
mod cx;
mod directive;
mod element;
mod expr;
mod forop;
mod html;
mod leaf;
mod once_memo;
mod show;
mod slot;
mod structural;
mod sugar;
mod table;
mod text;
mod vfor;
mod vtext;

pub use caps::LegacyCaps;

// The one-scanner rule (#4365): the S2 passes re-derive binding names
// with exactly the enumeration the lowering used, never a second one.
pub(crate) use expr::simple_identifier;
// The wrapper-key channel (P2-9 series 5): captured `<template v-if>`
// keys, folded into branch-key facts by the v-if pass.
pub use css::{lower_style_block, lower_style_block_in};
pub use structural::{ForWrapper, WrapperAttr, WrapperClass, WrapperKey, WrapperKeys};
// The one-rebuild rule (the same discipline): the text pass re-derives a
// compound's source with exactly the spelling the lowering minted.
pub use text::{TextPart, TextParts, rebuild_source};
pub(crate) use text::{legacy_slot_filler_needs_props_placeholder, legacy_slot_filler_text};

/// Feature bits the lowering observed while building S2.
///
/// These are not another tree scan: they are derived from the lowering's
/// own decision stream so the pass planner can skip artifact-irrelevant
/// mandatory passes without hiding traversal work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoweringFeatures {
    model_bindings: bool,
}

impl LoweringFeatures {
    pub const EMPTY: Self = Self {
        model_bindings: false,
    };

    #[must_use]
    pub const fn has_model_bindings(self) -> bool {
        self.model_bindings
    }

    pub(crate) const fn with_model_bindings(self) -> Self {
        Self {
            model_bindings: true,
        }
    }

    fn from_provenance(records: &[ProvenanceRecord]) -> Self {
        let mut features = Self::EMPTY;
        for record in records {
            if record.rule.as_str() == "lower.model" {
                features = features.with_model_bindings();
            }
        }
        features
    }
}

/// The S2 artifact one lowering produces: the op tree plus the three
/// fact channels, all live even when diagnostics are present (the
/// Lean-InfoTree survival property — kept fragments, not rollback).
pub struct Lowered<'a> {
    /// The compile arena this artifact was lowered into. The legalizing
    /// pass allocates rewritten expressions and inserted listeners here.
    pub allocator: &'a Allocator,
    /// The complete authored source that S0 spans are measured against.
    pub source: &'a str,
    /// The root region's ops, in document order.
    pub root: Region<'a>,
    /// How many ops were numbered (page order: every region op and
    /// attached binding, top to bottom). Equals the folio's `ops=`
    /// header.
    pub op_count: u32,
    /// Surface diagnostics (the converted tokenizer errors) followed by
    /// the lowering's own, in decision order. Owned and `'static` — they
    /// outlive the arena.
    pub diagnostics: StdVec<Diagnostic>,
    /// One record per lowering decision, in decision order, dropped
    /// content included (`vize_s2::provenance`).
    pub provenance: StdVec<ProvenanceRecord>,
    /// Hygiene scope facts per binding-introducing op
    /// (`vize_s2::scope`).
    pub scopes: SideTable<ScopeFacts>,
    /// The recorded parts of every merged text/interpolation run, keyed
    /// by its compound `ui.interpolation` op (P2-9 installment 4,
    /// [`lower::text`](text)); validated and consumed by `pass::text`.
    pub texts: SideTable<TextParts>,
    /// Captured `<template v-if>` wrapper keys, keyed by the `ui.if`
    /// op's page-order id (P2-9 series 5, [`WrapperKeys`]); folded into
    /// branch-key facts by `pass::vif`. `from_template` is the P2-11
    /// emit signal for template-fragment vs branch-root blocks.
    pub wrappers: SideTable<WrapperKeys>,
    /// Captured `<template v-for>` unwrap facts, keyed by the `ui.for`
    /// op's page-order id. Presence means the carrier was a template.
    pub for_wrappers: SideTable<ForWrapper>,
    /// Lowering-observed feature bits used by the S2 pass planner.
    pub features: LoweringFeatures,
    /// Vue dialect sugar the lowering (and the legalizing pass) consult.
    /// [`LegacyCaps::VUE3`] unless the caller used [`lower_with_caps`].
    pub caps: LegacyCaps,
}

/// Lower a parsed S1 tree (and the tokenizer errors its parse reported)
/// into S2.
///
/// Total over arbitrary input: malformed source arrives as typed S1
/// holes and lowers structurally; whatever the op family cannot carry
/// becomes a diagnostic plus a kept fragment. The only inherited
/// precondition is [`vize_s1::parse`]'s own u32-addressability
/// assertion, which every tree passed here has already satisfied.
#[must_use]
pub fn lower<'a>(
    allocator: &'a Allocator,
    tree: &SurfaceTree<'a>,
    errors: &[SurfaceError],
) -> Lowered<'a> {
    lower_with_caps(allocator, tree, errors, LegacyCaps::VUE3)
}

/// Lower with an explicit Vue dialect. Vue 3 is [`LegacyCaps::VUE3`]
/// (identical to [`lower`]); Vue 2 admits `.sync` / `slot-scope` /
/// pipe-filter payloads so the legalizing pass can rewrite them.
#[must_use]
pub fn lower_with_caps<'a>(
    allocator: &'a Allocator,
    tree: &SurfaceTree<'a>,
    errors: &[SurfaceError],
    caps: LegacyCaps,
) -> Lowered<'a> {
    let root = SourceRoot::new(tree.source).expect("vize_s1 accepted a u32-addressable source");
    lower_source_block_with_caps(allocator, tree, errors, root.whole_block(), caps)
}

pub(crate) fn lower_with_caps_and_comment_policy<'a>(
    allocator: &'a Allocator,
    tree: &SurfaceTree<'a>,
    errors: &[SurfaceError],
    caps: LegacyCaps,
    preserve_comments: bool,
    custom_element_patterns: &[String],
    custom_element_predicate: Option<fn(&str) -> bool>,
) -> Lowered<'a> {
    let root = SourceRoot::new(tree.source).expect("vize_s1 accepted a u32-addressable source");
    let custom_elements = LowerCustomElements {
        patterns: custom_element_patterns,
        predicate: custom_element_predicate,
    };
    lower_source_block_with_caps_and_comment_policy(
        allocator,
        tree,
        errors,
        root.whole_block(),
        caps,
        preserve_comments,
        custom_elements,
    )
}

/// Lower a parsed S1 source block while preserving file-absolute S0 spans.
#[must_use]
pub fn lower_source_block<'a>(
    allocator: &'a Allocator,
    tree: &SurfaceTree<'a>,
    errors: &[SurfaceError],
    block: SourceBlock<'a>,
) -> Lowered<'a> {
    lower_source_block_with_caps(allocator, tree, errors, block, LegacyCaps::VUE3)
}

/// [`lower_source_block`] under an explicit Vue dialect capability set.
#[must_use]
pub fn lower_source_block_with_caps<'a>(
    allocator: &'a Allocator,
    tree: &SurfaceTree<'a>,
    errors: &[SurfaceError],
    block: SourceBlock<'a>,
    caps: LegacyCaps,
) -> Lowered<'a> {
    lower_source_block_with_caps_and_comment_policy(
        allocator,
        tree,
        errors,
        block,
        caps,
        false,
        LowerCustomElements::default(),
    )
}

#[derive(Clone, Copy, Default)]
struct LowerCustomElements<'a> {
    patterns: &'a [String],
    predicate: Option<fn(&str) -> bool>,
}

fn lower_source_block_with_caps_and_comment_policy<'a>(
    allocator: &'a Allocator,
    tree: &SurfaceTree<'a>,
    errors: &[SurfaceError],
    block: SourceBlock<'a>,
    caps: LegacyCaps,
    preserve_comments: bool,
    custom_elements: LowerCustomElements<'_>,
) -> Lowered<'a> {
    debug_assert!(
        tree.source.as_ptr() == block.source().as_ptr()
            && tree.source.len() == block.source().len(),
        "the source block must be the exact string parsed into the S1 tree"
    );
    let mut cx = cx::Cx::with_source_block_and_comment_policy(
        allocator,
        block,
        caps,
        preserve_comments,
        custom_elements.patterns,
        custom_elements.predicate,
    );
    for error in errors {
        cx.diagnostics.push(Diagnostic::new(
            Severity::Error,
            Stage::Surface,
            surface_error_span(block, error.offset),
            String::from(error.code.message()),
        ));
    }
    let ops = structural::lower_children(&mut cx, &tree.children, Namespace::Html);
    let features = LoweringFeatures::from_provenance(&cx.provenance);
    Lowered {
        allocator,
        source: block.root_source(),
        root: Region { ops },
        op_count: cx.op_count(),
        diagnostics: cx.diagnostics,
        provenance: cx.provenance,
        scopes: cx.scopes,
        texts: cx.texts,
        wrappers: cx.wrappers,
        for_wrappers: cx.for_wrappers,
        features,
        caps,
    }
}

fn surface_error_span(block: SourceBlock<'_>, offset: u32) -> Span {
    let absolute = block.start().saturating_add(offset);
    let hole = block.zero_width_at(absolute);
    block.span_of(hole).unwrap_or_else(|| {
        debug_assert!(
            false,
            "SourceBlock::zero_width_at returned a slice outside its block"
        );
        Span::new(block.start(), block.start())
    })
}

impl fmt::Debug for Lowered<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lowered")
            .field("root", &self.root)
            .field("op_count", &self.op_count)
            .field("diagnostics", &self.diagnostics)
            .field("provenance", &self.provenance)
            .field("scopes", &self.scopes)
            .field("texts", &self.texts)
            .field("wrappers", &self.wrappers)
            .field("for_wrappers", &self.for_wrappers)
            .field("features", &self.features)
            .field("caps", &self.caps)
            .finish_non_exhaustive()
    }
}
