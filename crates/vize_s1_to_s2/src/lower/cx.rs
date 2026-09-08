//! The lowering context: id minting, span recovery, and the three fact
//! channels (diagnostics, provenance, scopes).
//!
//! # Ids are page order
//!
//! S2 node ids are dense and page-ordered — every op line top to bottom,
//! attached bindings between their owner's line and its children
//! (`folio-format.md`, "Node numbering"; `vize_s2::verify` checks
//! references against exactly that numbering). [`Cx::mint_op`] therefore
//! runs in **print order** during the walk: an op's id is minted when its
//! line is decided, bindings as they attach, children after. The pinned
//! law is `Cx::op_count == S2Folio::of(root).op_count()`, tested per
//! fixture.
//!
//! Exhaustion is a diagnostic, not a panic (the `NodeId::from_index`
//! contract): past `u32::MAX - 1` ops the context reports once and stops
//! minting; side tables simply stop gaining keys while the op tree stays
//! total.
//! [`S2Folio`]: vize_s2::folio::S2Folio

use alloc::vec::Vec;

use vize_davinci::diagnostic::{Diagnostic, Severity, Stage};
use vize_davinci::id::NodeId;
use vize_davinci::side_table::SideTable;
use vize_s0::{Allocator, SourceBlock, Span, String};
use vize_s1::{Element, ElementClose, Token};
use vize_s2::provenance::ProvenanceRecord;
use vize_s2::scope::{ScopeFacts, ScopeTag};

mod custom_element;
mod for_parts;
mod span;

pub(crate) struct Cx<'a> {
    pub allocator: &'a Allocator,
    block: SourceBlock<'a>,
    /// The complete authored source that S0 spans are measured against.
    pub source: &'a str,
    next_op: u32,
    exhausted: bool,
    next_scope: u32,
    /// How many `<pre>` ancestors the walk is inside; condensing is
    /// suppressed for those subtrees (`lower::text`).
    condense_depth: u32,
    /// How many `v-pre` ancestors the walk is inside; interpolations are
    /// inert authored text for those subtrees.
    v_pre_depth: u32,
    /// Whether ordinary comments lower to `ui.comment`.
    preserve_comments: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub provenance: Vec<ProvenanceRecord>,
    pub scopes: SideTable<ScopeFacts>,
    pub texts: SideTable<super::text::TextParts>,
    pub for_facts: SideTable<super::forop::ForParts>,
    pub if_facts: SideTable<super::if_keys::IfFacts>,
    pub wrappers: SideTable<super::structural::WrapperKeys>,
    pub for_wrappers: SideTable<super::structural::ForWrapper>,
    pub caps: super::caps::LegacyCaps,
    /// Op families this lowering built, set where the op is minted —
    /// see `lower::features`.
    pub features: super::features::LoweringFeatures,
    custom_element_patterns: Vec<String>,
    custom_element_predicate: Option<fn(&str) -> bool>,
}

impl<'a> Cx<'a> {
    pub(crate) fn with_source_block_and_comment_policy(
        allocator: &'a Allocator,
        block: SourceBlock<'a>,
        caps: super::caps::LegacyCaps,
        preserve_comments: bool,
        custom_element_patterns: &[String],
        custom_element_predicate: Option<fn(&str) -> bool>,
    ) -> Self {
        Self {
            allocator,
            block,
            source: block.root_source(),
            next_op: 0,
            exhausted: false,
            next_scope: 0,
            condense_depth: 0,
            v_pre_depth: 0,
            preserve_comments,
            diagnostics: Vec::new(),
            provenance: Vec::new(),
            scopes: SideTable::new(),
            texts: SideTable::new(),
            for_facts: SideTable::new(),
            if_facts: SideTable::new(),
            wrappers: SideTable::new(),
            for_wrappers: SideTable::new(),
            caps,
            features: super::features::LoweringFeatures::EMPTY,
            custom_element_patterns: custom_element_patterns.to_vec(),
            custom_element_predicate,
        }
    }

    /// Record that this lowering built an op of `family`.
    ///
    /// Called where the op is minted, never from a rule name: a decision
    /// that *failed* still leaves an op behind (a malformed `v-for` keeps
    /// its `ui.for` under the escape), and the planner must see it.
    pub(crate) fn observe(&mut self, family: super::features::OpFamily) {
        self.features = self.features.observing(family);
    }

    /// Whether the walk is inside a condense-suppressing subtree.
    pub(crate) fn condense_suppressed(&self) -> bool {
        self.condense_depth > 0
    }

    /// Whether the walk is inside a `v-pre` subtree.
    pub(crate) fn v_pre_suppressed(&self) -> bool {
        self.v_pre_depth > 0
    }

    pub(crate) fn preserve_comments(&self) -> bool {
        self.preserve_comments
    }

    /// Enter/leave a condense-suppressing `<pre>` around its children.
    pub(crate) fn push_condense_suppression(&mut self) {
        self.condense_depth = self.condense_depth.saturating_add(1);
    }

    pub(crate) fn pop_condense_suppression(&mut self) {
        self.condense_depth = self.condense_depth.saturating_sub(1);
    }

    /// Enter/leave a `v-pre` subtree around its children.
    pub(crate) fn push_v_pre_suppression(&mut self) {
        self.v_pre_depth = self.v_pre_depth.saturating_add(1);
    }

    pub(crate) fn pop_v_pre_suppression(&mut self) {
        self.v_pre_depth = self.v_pre_depth.saturating_sub(1);
    }

    /// Attach a merged run's recorded parts to its compound op, when the
    /// op has an id (the `attach_scope` exhaustion rule).
    pub(crate) fn attach_texts(
        &mut self,
        node: Option<NodeId>,
        parts: super::text::TextParts,
        span: Span,
        source: &str,
    ) {
        self.observe(super::features::OpFamily::TextCompound);
        if let Some(id) = node {
            parts.assert_compound_laws(id, span, source);
            let before = vize_s0::cstr!("parts={}", parts.parts.len());
            let dynamic = parts.dynamic_count();
            self.record(
                "lower.text-fact",
                node,
                before.as_str(),
                vize_s0::cstr!(
                    "fact static={} dynamic={dynamic}",
                    parts.parts.len() - dynamic
                ),
                span,
            );
            self.texts.insert(id, parts);
        }
    }

    /// Attach captured wrapper keys to their `ui.if` op, when the op has
    /// an id (the `attach_scope` exhaustion rule).
    pub(crate) fn attach_wrappers(
        &mut self,
        node: Option<NodeId>,
        keys: super::structural::WrapperKeys,
    ) {
        if let Some(id) = node {
            self.wrappers.insert(id, keys);
        }
    }

    /// Attach a `<template v-for>` unwrap fact to its `ui.for` op.
    pub(crate) fn attach_for_wrapper(
        &mut self,
        node: Option<NodeId>,
        wrapper: super::structural::ForWrapper,
    ) {
        if let Some(id) = node {
            self.for_wrappers.insert(id, wrapper);
        }
    }

    /// The next page-order op id, or `None` after exhaustion (reported
    /// once, as a diagnostic).
    pub(crate) fn mint_op(&mut self) -> Option<NodeId> {
        if self.exhausted {
            return None;
        }
        match NodeId::from_index(self.next_op) {
            Some(id) => {
                self.next_op += 1;
                Some(id)
            }
            None => {
                self.exhausted = true;
                self.error(
                    Span::new(0, 0),
                    String::from(
                        "the artifact exhausted its node ids; facts past this point are not keyed",
                    ),
                );
                None
            }
        }
    }

    pub(crate) fn op_count(&self) -> u32 {
        self.next_op
    }

    pub(crate) fn mint_scope(&mut self) -> ScopeTag {
        let tag = ScopeTag::from_index(self.next_scope);
        self.next_scope = self.next_scope.saturating_add(1);
        tag
    }

    /// Attach scope facts to an op, when the op has an id (after id
    /// exhaustion the tree stays total and only the keying stops).
    pub(crate) fn attach_scope(&mut self, node: Option<NodeId>, facts: ScopeFacts) {
        if let Some(id) = node {
            self.scopes.insert(id, facts);
        }
    }

    /// Byte offset of `slice` inside the authored source. Every S1 string
    /// is a slice of the one source (`vize_s1`'s P1-10 contract), so
    /// this is pointer arithmetic, never a search.
    pub(crate) fn offset(&self, slice: &str) -> u32 {
        match self.block.offset_of(slice) {
            Some(offset) => offset,
            None => {
                debug_assert!(
                    false,
                    "S1 handed the lowering a string that is not a block slice"
                );
                self.block.start()
            }
        }
    }

    pub(crate) fn span_of(&self, slice: &str) -> Span {
        self.block.span_of(slice).unwrap_or_else(|| {
            let start = self.offset(slice);
            Span::new(start, start.saturating_add(slice.len() as u32))
        })
    }

    /// The span of a token's own text (`leading` excluded; a `Missing`
    /// token is zero-width at the offset where its syntax belongs).
    pub(crate) fn token_span(&self, token: &Token<'a>) -> Span {
        self.span_of(token.text)
    }

    /// A zero-width **source** slice at `offset` (snapped into range and
    /// onto a char boundary), for positions whose syntax was never
    /// authored — an expression hole still needs a slice the span
    /// machinery can locate.
    pub(crate) fn hole_at(&self, offset: u32) -> &'a str {
        self.block.zero_width_at(offset)
    }

    pub(crate) fn error(&mut self, span: Span, message: String) {
        self.diagnostics.push(Diagnostic::new(
            Severity::Error,
            Stage::Semantic,
            span,
            message,
        ));
    }

    /// Render an S1 typed hole into the unified channel. The tokenizer
    /// never reports a missing end tag (end-tag matching is tree
    /// construction, not lexing), so the `ElementClose::Missing` hole
    /// becomes a diagnostic exactly here — the P2-7 hand-off's second
    /// half. One chokepoint per lowered element; the exact relief
    /// `MissingEndTag` message, pointing at the open tag.
    pub(crate) fn report_missing_close(&mut self, element: &Element<'a>) {
        if matches!(element.close, ElementClose::Missing) {
            let span = self.token_span(&element.open.lt_name);
            self.diagnostics.push(Diagnostic::new(
                Severity::Error,
                Stage::Surface,
                span,
                String::from("Element is missing end tag."),
            ));
        }
    }

    pub(crate) fn info(&mut self, span: Span, message: String) {
        self.diagnostics.push(Diagnostic::new(
            Severity::Info,
            Stage::Semantic,
            span,
            message,
        ));
    }

    /// One provenance record, in decision order (the survival law:
    /// `node: None` + empty `after` is a decision that produced nothing).
    pub(crate) fn record(
        &mut self,
        rule: &'static str,
        node: Option<NodeId>,
        before: &str,
        after: String,
        span: Span,
    ) {
        self.provenance.push(ProvenanceRecord {
            rule: String::from(rule),
            node,
            before: String::from(before),
            after,
            span,
        });
    }
}

pub(crate) use span::{attr_slice, attr_span, element_span};
