//! The exclusion predicates of the P2-15 mutators.
//!
//! Split from `sites.rs` under the 350-line budget: the walk enumerates,
//! this module judges. Each returned reason is a counted skip; the
//! semantic argument for every "allow" lives in `mutators.rs`.

use vize_s1::{Attribute, Element, TokenStatus};

use super::sites::{Flags, Parent, is_vue_ws};

/// Elements where DOM attribute-application order is observable
/// (`value`/`type` interplay on form controls, resource selection on
/// media, `loading`/`src` on images), so static-attribute reorder skips.
const ATTR_ORDER_TAGS: [&str; 16] = [
    "input", "select", "option", "textarea", "progress", "meter", "img", "video", "audio",
    "source", "track", "script", "style", "link", "iframe", "embed",
];

pub fn is_branch_name(name: &str) -> bool {
    matches!(name, "v-if" | "v-else-if" | "v-else")
}

pub fn has_attr_named(element: &Element<'_>, name: &str) -> bool {
    element.open.attrs.iter().any(|attr| attr.name.text == name)
}

pub fn has_slot_attr(element: &Element<'_>) -> bool {
    element.open.attrs.iter().any(|attr| {
        attr.name.text == "v-slot"
            || attr.name.text.starts_with("v-slot:")
            || attr.name.text.starts_with('#')
    })
}

fn is_branch_key_candidate(name: &str) -> bool {
    matches!(name, "key" | ":key" | ".key")
        || name.starts_with(":key.")
        || name == "v-bind:key"
        || name.starts_with("v-bind:key.")
        || name.starts_with(":[")
        || name.starts_with("v-bind:[")
}

/// A directive-shaped attribute name (any form the S1→S2 classifier
/// treats as non-static, plus the `.prop` shorthand).
fn is_directive_name(name: &str) -> bool {
    name.starts_with("v-")
        || name.starts_with(':')
        || name.starts_with('@')
        || name.starts_with('#')
        || name.starts_with('.')
}

/// A fully-authored attribute whose swap re-renders into the same two
/// attributes: present tokens, a non-empty all-whitespace gap before the
/// name, and either no value or a both-quotes-present quoted value.
fn well_formed_for_swap(attr: &Attribute<'_>) -> Option<&'static str> {
    if attr.name.status != TokenStatus::Present || attr.name.text.is_empty() {
        return Some("name-hole");
    }
    if attr.name.leading.is_empty() || !attr.name.leading.chars().all(is_vue_ws) {
        return Some("attr-gap");
    }
    match (&attr.eq, &attr.value) {
        (_, None) => None,
        (Some(_), Some(value)) => {
            let quoted = value.open_quote.is_some()
                && value
                    .close_quote
                    .as_ref()
                    .is_some_and(|quote| quote.status == TokenStatus::Present);
            if !quoted || value.content.status != TokenStatus::Present {
                return Some("unquoted-or-hole-value");
            }
            None
        }
        (None, Some(_)) => Some("recovered-value"),
    }
}

pub fn reorder_skip(element: &Element<'_>, index: usize, flags: Flags) -> Option<&'static str> {
    if flags.rawtext {
        return Some("rawtext-content");
    }
    if flags.v_pre {
        return Some("v-pre-subtree");
    }
    if ATTR_ORDER_TAGS.contains(&element.tag()) {
        return Some("attr-order-stateful-tag");
    }
    let first = &element.open.attrs[index];
    let second = &element.open.attrs[index + 1];
    for attr in [first, second] {
        if is_directive_name(attr.name.text) {
            return Some("directive-attr");
        }
        if let Some(reason) = well_formed_for_swap(attr) {
            return Some(reason);
        }
        let lowered = attr.name.text.to_ascii_lowercase();
        if lowered == "class" || lowered == "style" {
            return Some("merge-order-family");
        }
    }
    if first.name.text.eq_ignore_ascii_case(second.name.text) {
        return Some("duplicate-name");
    }
    None
}

pub fn wrap_skip(
    element: &Element<'_>,
    index: usize,
    flags: Flags,
    parent: Parent,
) -> Option<&'static str> {
    if flags.rawtext {
        return Some("rawtext-content");
    }
    if flags.v_pre || has_attr_named(element, "v-pre") {
        return Some("v-pre-subtree");
    }
    if element.tag() == "template" {
        return Some("template-tag");
    }
    if element
        .open
        .attrs
        .iter()
        .enumerate()
        .any(|(attr_index, attr)| attr_index != index && is_branch_key_candidate(attr.name.text))
    {
        return Some("branch-key-carrier");
    }
    if parent.component || parent.slot_template {
        return Some("slot-content");
    }
    if has_slot_attr(element) {
        return Some("slot-carrier");
    }
    let branches = element
        .open
        .attrs
        .iter()
        .filter(|attr| is_branch_name(attr.name.text))
        .count();
    if branches != 1 {
        return Some("duplicate-branch-attr");
    }
    let attr = &element.open.attrs[index];
    if let Some(reason) = well_formed_for_swap(attr) {
        return Some(reason);
    }
    if attr.name.text != "v-else"
        && attr
            .value
            .as_ref()
            .is_none_or(|value| value.content.text.trim().is_empty())
    {
        return Some("missing-expression");
    }
    if element.open.gt.status != TokenStatus::Present {
        return Some("open-tag-hole");
    }
    if matches!(element.close, vize_s1::ElementClose::Missing) {
        return Some("missing-end-tag");
    }
    None
}

pub fn merge_skip(
    source: &str,
    first: &vize_s1::Token<'_>,
    second: &vize_s1::Token<'_>,
    flags: Flags,
) -> Option<&'static str> {
    if flags.rawtext {
        return Some("rawtext-content");
    }
    if flags.v_pre {
        return Some("v-pre-subtree");
    }
    if first.status != TokenStatus::Present || second.status != TokenStatus::Present {
        return Some("text-hole");
    }
    if !second.leading.is_empty() {
        return Some("gap-bytes");
    }
    let base = source.as_ptr() as usize;
    let first_end = first.text.as_ptr() as usize + first.text.len() - base;
    let second_start = second.text.as_ptr() as usize - base;
    if first_end != second_start {
        return Some("non-contiguous");
    }
    None
}
