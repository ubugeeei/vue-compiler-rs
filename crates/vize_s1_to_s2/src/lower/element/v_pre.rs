//! Freezing an element's attributes under `v-pre`.
//!
//! The rule has two halves, because the shipped tokenizer enters `v-pre`
//! only once it has finished reading the opening tag that carries it:
//!
//! - **Inside** the subtree the tokenizer never split a name, so every
//!   attribute already reads as the plain one it was authored as and
//!   `Analyzed::freeze_as_authored` simply reclassifies it.
//! - **On the element that opens the subtree** the names arrived split
//!   into head, argument and modifiers, and the `v-pre` rewrite
//!   reassembles them — which is where [`frozen_name`] comes in.

use vize_s0::StringBuilder;

use super::super::cx::Cx;
use super::super::directive::Directive;

/// One attribute's name as the shipped lane freezes it on the element
/// that *opens* a `v-pre` subtree.
///
/// The tokenizer read the spelling as a directive — head, argument,
/// modifiers — before `v-pre` took effect, and the `v-pre` rewrite
/// reassembles the name from those parts. Separators do not survive
/// that: the `:` before an argument, the `[`/`]` around a dynamic one,
/// and every `.modifier` are all lost, so `v-bind:x` freezes as
/// `v-bindx`, `:[d]` as `:d` and `v-my:a.m` as `v-mya`. A shorthand
/// prefix is part of the head and stays, so `:x`, `@c`, `.p` and `^a`
/// are unchanged. `@vue/compiler-dom` does exactly the same.
pub(super) fn frozen_name<'a>(
    cx: &Cx<'a>,
    authored: &'a str,
    directive: &Directive<'a>,
) -> &'a str {
    let drops_nothing = !authored[1..].contains(':')
        && !authored.contains('[')
        && !authored.contains(']')
        && directive.modifiers.is_empty();
    if drops_nothing {
        // The common shape: nothing to rebuild, so keep borrowing.
        return authored;
    }
    let mut trimmed = authored;
    for modifier in &directive.modifiers {
        if let Some(rest) = trimmed.strip_suffix(modifier)
            && let Some(rest) = rest.strip_suffix('.')
        {
            trimmed = rest;
        }
    }
    let mut out = StringBuilder::with_capacity_in(trimmed.len(), cx.allocator);
    for (index, ch) in trimmed.char_indices() {
        match ch {
            ':' if index > 0 => {}
            '[' | ']' => {}
            other => out.push(other),
        }
    }
    out.into_str()
}
