//! The four P2-15 mutators, each with its equivalence justification.
//!
//! Every mutator here claims "a Vue 3 compiler must give this mutant the
//! same meaning as the original". An unjustified mutator is an oracle
//! asserting a wrong expected value — worse than no assertion (assurance
//! §4) — so each claim below is argued against Vue's semantics with the
//! code that implements those semantics cited, and everything the
//! argument does not cover is excluded by a predicate in `sites.rs`.
//!
//! # Attribute reorder ([`Kind::Reorder`])
//!
//! **Claim:** swapping two adjacent, fully-authored, *static* attributes
//! with distinct case-insensitive names preserves Vue semantics.
//! **Argument:** HTML treats an element's attributes as a name→value map;
//! Vue lowers static attributes into the props object, where two distinct
//! keys commute — only patch-application order changes, and that is
//! observable only through duplicate keys (first-wins), the
//! `class`/`style` merge families, and DOM-stateful attribute pairs on
//! form/media/resource elements. **Exclusions** (`sites::reorder_skip`):
//! directive-shaped names (`v-*`/`:`/`@`/`#`/`.` — binding order against
//! `v-bind="obj"` spreads and directive application order are
//! order-sensitive), equal names, `class`/`style` in any spelling, the
//! `ATTR_ORDER_TAGS` element list (`value`/`type` interplay and friends),
//! unquoted or hole-bearing values (the swap must re-render into the same
//! two attributes), and `v-pre`/rawtext regions. **Declared
//! normalization:** the S2 folio records attributes in authored order
//! (`crates/vize_s1_to_s2/src/lower/element.rs:147-168`), so the oracle
//! compares with attribute lines sorted — the canonical quotient by
//! exactly the permutation the mutator induces; absolute order stays
//! pinned by the P2-8 exact-equality suites.
//!
//! # Pass-through `<template>` wrap ([`Kind::Wrap`])
//!
//! **Claim:** moving a `v-if`/`v-else-if`/`v-else` from an element onto a
//! freshly inserted `<template>` wrapper around that element preserves
//! Vue semantics. **Argument:** this is Vue's documented invisible-
//! wrapper pattern — a `<template>` carrying a structural directive
//! renders no node of its own; the branch resolves to the wrapped
//! element's node in both spellings, root position included, so root
//! counting and attribute fallthrough see the same node. The lowering
//! encodes the same rule: a branch-carrying `<template>` unwraps into its
//! branch region (`crates/vize_s1_to_s2/src/lower/structural.rs:213-217`,
//! the P2-8 unwrap site). A **bare** `<template>` is *not* pass-through —
//! Vue renders it as a real template element — which is why this mutator
//! only ever creates a wrapper that carries the moved branch directive.
//! **Exclusions** (`sites::wrap_skip`): `<template>` targets (the
//! directive would leave a bare template behind), elements carrying a
//! branch-carrier `key` (the move would turn the branch key into a child
//! key), elements carrying `v-slot`/`#…` and direct children of
//! components or slot templates (slot targeting and slot-content
//! assignment are position-sensitive),
//! a second branch directive on the element (the leftover would form a
//! new chain), missing expressions, unterminated tags, `v-pre`/rawtext.
//! **Normalization:** none beyond span elision — the folios must match
//! structurally.
//!
//! # Text-node split/merge ([`Kind::Split`], [`Kind::Merge`])
//!
//! **Claim:** replacing one S1 text node by two adjacent text nodes
//! covering the same bytes (and the inverse) preserves Vue semantics.
//! **Argument:** adjacent template text renders as one DOM text run; the
//! shipped pipeline merges consecutive text/interpolation children into
//! a single call at codegen
//! (`crates/vize_atelier_core/src/codegen/children.rs`), and since P2-9
//! installment 4 the S2 lowering fuses them at conversion
//! (`crates/vize_s1_to_s2/src/lower/text.rs`), so node granularity of
//! text is representation, not meaning. These two mutate the **S1 tree**
//! rather than the source — no source spelling can split a maximal text
//! run — with both halves kept as genuine source subslices, so spans stay
//! authored. **Exclusions** (`sites.rs`): rawtext/`v-pre` content, text
//! with no interior char boundary; merge additionally requires contiguous
//! slices with no gap bytes (recovered junk would become visible text).
//! **Declared normalization:** none beyond span elision — the P2-15
//! `merge_text` quotient was **ratcheted out** when P2-9 installment 4
//! absorbed the merge into the lowering itself
//! (`crates/vize_s1_to_s2/src/lower/text.rs`): adjacent text now fuses
//! before ids mint, so the mutant lowers to the original's artifact
//! with no declared rule at all. Split additionally proves the exact
//! inverse: re-merging the split tree must reproduce the original
//! `Full`-mode folio byte-for-byte.
//!
//! # Whitespace-insignificant edits ([`Kind::Whitespace`])
//!
//! **Claim:** replacing one maximal whitespace run in template text by a
//! different run over Vue's whitespace alphabet, preserving
//! newline-presence, preserves Vue semantics under the default
//! `condense` strategy. **Argument:** Vue's condense rules
//! (`crates/vize_armature/src/parser/whitespace.rs`, matching
//! `@vue/compiler-sfc`) erase exactly this difference: whitespace-only
//! nodes between non-text siblings are removed when they contain a
//! newline (`:115-118` — newline-presence must therefore be preserved,
//! or removal flips to a kept space) and condensed to one space
//! otherwise; interior runs of mixed text collapse to one space
//! (`:22-61`); leading/trailing whitespace-only children vanish
//! (`:74-95`). The alphabet is exactly `[ \t\n\f\r]` (`:12-16`), so
//! replacement runs drawn from it cannot decode into or out of entity
//! text. **Exclusions** (`sites.rs`): `<pre>` content (the strategy
//! skips pre subtrees, `:155-161`), rawtext content (another language's
//! whitespace), `v-pre` subtrees. **Declared normalization:** none
//! beyond span elision — the P2-15 condense mirror was **ratcheted
//! out** when P2-9 installment 4 absorbed the condense into the
//! lowering (`crates/vize_s1_to_s2/src/lower/text.rs`, rule-for-rule
//! the cited armature algorithm): both sides now lower pre-condensed,
//! so the quotient the justification licenses is already the artifact.

use vize_s0::{Allocator, Box, String, StringBuilder, Vec as ArenaVec};
use vize_s1::{
    Attribute, CloseTag, Element, ElementClose, OpenTag, SurfaceChild, SurfaceTree, Token,
    TokenStatus, render,
};

use super::sites::{Detail, Site};

/// How a mutant reaches the oracle.
pub enum Mutant {
    /// The mutated tree re-rendered to source; run the whole pipeline on
    /// it (parse included).
    Source(String),
    /// The tree was mutated in place with genuine source subslices;
    /// lower it directly (no source spelling exists for this mutant).
    InPlace,
}

fn tok<'a>(text: &'a str) -> Token<'a> {
    Token {
        leading: "",
        text,
        status: TokenStatus::Present,
    }
}

fn render_to_string(tree: &SurfaceTree<'_>) -> String {
    let mut out = String::default();
    render(tree, &mut |piece| out.push_str(piece));
    out
}

fn child_at<'t, 'a>(tree: &'t mut SurfaceTree<'a>, path: &[usize]) -> &'t mut SurfaceChild<'a> {
    let (first, rest) = path.split_first().expect("site path is never empty");
    let mut child = &mut tree.children[*first];
    for index in rest {
        let SurfaceChild::Element(element) = child else {
            panic!("site path expects an element");
        };
        child = &mut element.children[*index];
    }
    child
}

fn element_at<'t, 'a>(tree: &'t mut SurfaceTree<'a>, path: &[usize]) -> &'t mut Element<'a> {
    match child_at(tree, path) {
        SurfaceChild::Element(element) => element,
        _ => panic!("site path expects an element"),
    }
}

/// Apply `site` to a freshly parsed copy of the original tree.
pub fn apply<'a>(allocator: &'a Allocator, tree: &mut SurfaceTree<'a>, site: &Site) -> Mutant {
    match &site.detail {
        Detail::AttrPair { index } => {
            let element = element_at(tree, &site.path);
            element.open.attrs.swap(*index, *index + 1);
            Mutant::Source(render_to_string(tree))
        }
        Detail::BranchAttr { index } => {
            let attr = element_at(tree, &site.path).open.attrs.remove(*index);
            wrap_in_template(allocator, tree, &site.path, attr);
            Mutant::Source(render_to_string(tree))
        }
        Detail::SplitAt { at } => {
            split_text(tree, &site.path, *at);
            Mutant::InPlace
        }
        Detail::MergeNext => {
            merge_text(tree, &site.path);
            Mutant::InPlace
        }
        Detail::WsRun {
            start,
            len,
            newline,
        } => {
            let SurfaceChild::Text(token) = child_at(tree, &site.path) else {
                panic!("whitespace site expects a text node");
            };
            let text = token.text;
            let run = &text[*start..*start + *len];
            let replacement = ws_replacement(run, *newline);
            let mut built =
                StringBuilder::with_capacity_in(text.len() - *len + replacement.len(), allocator);
            built.push_str(&text[..*start]);
            built.push_str(replacement);
            built.push_str(&text[*start + *len..]);
            token.text = built.into_str();
            Mutant::Source(render_to_string(tree))
        }
    }
}

/// The deterministic replacement run: a different spelling over the Vue
/// whitespace alphabet with the same newline-presence.
pub fn ws_replacement(run: &str, newline: bool) -> &'static str {
    let primary = if newline { "\n\t " } else { "\t\t" };
    if primary == run {
        if newline { "\n  " } else { " \t" }
    } else {
        primary
    }
}

fn wrap_in_template<'a>(
    allocator: &'a Allocator,
    tree: &mut SurfaceTree<'a>,
    path: &[usize],
    attr: Attribute<'a>,
) {
    let slot = child_at(tree, path);
    let inner = core::mem::replace(slot, SurfaceChild::Text(tok("")));
    let mut attrs = ArenaVec::new_in(&allocator);
    attrs.push(attr);
    let mut children = ArenaVec::new_in(&allocator);
    children.push(inner);
    let wrapper = Element {
        open: OpenTag {
            lt_name: tok("<template"),
            attrs,
            slash: None,
            gt: tok(">"),
        },
        children,
        close: ElementClose::Present(CloseTag {
            lt_slash_name: tok("</template"),
            gt: tok(">"),
        }),
    };
    *slot = SurfaceChild::Element(Box::new_in(wrapper, &allocator));
}

/// Split the text node at `path` into two nodes covering the same bytes;
/// both slices stay genuine source subslices, so spans stay authored.
pub fn split_text<'a>(tree: &mut SurfaceTree<'a>, path: &[usize], at: usize) {
    let (index, parent_path) = path.split_last().expect("site path is never empty");
    let (leading, text) = {
        let SurfaceChild::Text(token) = child_at(tree, path) else {
            panic!("split site expects a text node");
        };
        (token.leading, token.text)
    };
    let children = children_of(tree, parent_path);
    children[*index] = SurfaceChild::Text(Token {
        leading,
        text: &text[..at],
        status: TokenStatus::Present,
    });
    children.insert(*index + 1, SurfaceChild::Text(tok(&text[at..])));
}

/// Merge the text node at `path` with its following text sibling; the
/// merged slice is re-cut from the source, so it stays genuine.
pub fn merge_text<'a>(tree: &mut SurfaceTree<'a>, path: &[usize]) {
    let source = tree.source;
    let (index, parent_path) = path.split_last().expect("site path is never empty");
    let children = children_of(tree, parent_path);
    let (first, second) = match (&children[*index], &children[*index + 1]) {
        (SurfaceChild::Text(first), SurfaceChild::Text(second)) => (*first, *second),
        _ => panic!("merge site expects two adjacent text nodes"),
    };
    let base = source.as_ptr() as usize;
    let start = first.text.as_ptr() as usize - base;
    let end = second.text.as_ptr() as usize + second.text.len() - base;
    let merged = Token {
        leading: first.leading,
        text: &source[start..end],
        status: TokenStatus::Present,
    };
    children[*index] = SurfaceChild::Text(merged);
    children.remove(*index + 1);
}

fn children_of<'t, 'a>(
    tree: &'t mut SurfaceTree<'a>,
    parent_path: &[usize],
) -> &'t mut ArenaVec<'a, SurfaceChild<'a>> {
    if parent_path.is_empty() {
        &mut tree.children
    } else {
        &mut element_at(tree, parent_path).children
    }
}
