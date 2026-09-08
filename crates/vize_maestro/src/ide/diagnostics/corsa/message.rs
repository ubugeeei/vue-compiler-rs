//! Vue-flavored rewriting of raw Corsa/TypeScript diagnostic messages.

use vize_canon::batch::restore_virtual_vue_specifiers;
use vize_s0::cstr;

/// Rewrite a Corsa diagnostic message with a Vue-flavored hint when the
/// raw TypeScript phrasing has a more actionable Vue interpretation.
///
/// The original wording is preserved as the prefix so the user can still see
/// what TypeScript reported. The added hint points at the most common Vue
/// cause for that error shape.
pub(super) fn rewrite_corsa_message(message: &str, authored_source: &str) -> String {
    let normalized_message =
        strip_corsa_overlay_paths(&restore_virtual_vue_specifiers(message, authored_source));
    let message = normalized_message.as_str();

    if let Some(prop) = property_does_not_exist_property(message)
        && prop != "value"
    {
        return cstr!(
            "{message}\n\nIf you intended to read the reactive value, try `.value`. (vize/types)"
        )
        .into();
    }
    if message.starts_with("Type 'Ref<") && message.contains("is not assignable to type") {
        return cstr!(
            "{message}\n\nDid you forget `.value`? Vue refs need to be unwrapped in script context. (vize/types)"
        ).into();
    }
    message.to_string()
}

fn strip_corsa_overlay_paths(message: &str) -> String {
    const MARKER: &str = "/node_modules/.vize/corsa/";
    const OVERLAYS: &str = "/overlays";

    let mut rewritten = String::with_capacity(message.len());
    let mut cursor = 0;
    let mut changed = false;

    while let Some(relative_marker) = message[cursor..].find(MARKER) {
        let marker = cursor + relative_marker;
        let after_marker = marker + MARKER.len();
        let Some(relative_overlays) = message[after_marker..].find(OVERLAYS) else {
            rewritten.push_str(&message[cursor..after_marker]);
            cursor = after_marker;
            continue;
        };
        let overlays = after_marker + relative_overlays;
        let overlay_source_start = overlays + OVERLAYS.len();
        if !message[overlay_source_start..].starts_with('/') {
            rewritten.push_str(&message[cursor..overlay_source_start]);
            cursor = overlay_source_start;
            continue;
        }

        let path_start = previous_path_boundary(message, marker);
        let path_end = next_path_boundary(message, overlay_source_start);
        rewritten.push_str(&message[cursor..path_start]);
        rewritten.push_str(&message[overlay_source_start..path_end]);
        cursor = path_end;
        changed = true;
    }

    if !changed {
        return message.to_string();
    }

    rewritten.push_str(&message[cursor..]);
    rewritten
}

fn previous_path_boundary(message: &str, before: usize) -> usize {
    message[..before]
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            is_path_boundary(character).then_some(index + character.len_utf8())
        })
        .unwrap_or(0)
}

fn next_path_boundary(message: &str, after: usize) -> usize {
    message[after..]
        .char_indices()
        .find_map(|(offset, character)| is_path_boundary(character).then_some(after + offset))
        .unwrap_or(message.len())
}

fn is_path_boundary(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '\'' | '"' | '`' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
        )
}

/// Extract the property name from a TS7053/TS2339 "Property 'X' does not
/// exist on type 'Y'" message. Returns `None` for unrelated messages.
fn property_does_not_exist_property(message: &str) -> Option<&str> {
    let head = "Property '";
    let after = message.strip_prefix(head)?;
    let end = after.find('\'')?;
    let rest = &after[end..];
    if !rest.starts_with("' does not exist") {
        return None;
    }
    Some(&after[..end])
}

#[cfg(test)]
mod hint_tests {
    use vize_s0::cstr;

    use super::{
        property_does_not_exist_property, rewrite_corsa_message, strip_corsa_overlay_paths,
    };

    #[test]
    fn rewrites_property_does_not_exist_with_value_hint() {
        let original = "Property 'toFixed' does not exist on type 'Ref<number>'.";
        let rewritten = rewrite_corsa_message(original, "");
        assert!(rewritten.contains(original));
        assert!(
            rewritten.contains(".value"),
            "expected a .value hint, got {rewritten:?}"
        );
    }

    #[test]
    fn leaves_known_property_value_alone() {
        // We don't want to suggest `.value` on a `.value` access — that's
        // already what the user wrote.
        let original = "Property 'value' does not exist on type 'unknown'.";
        let rewritten = rewrite_corsa_message(original, "");
        assert_eq!(rewritten, original);
    }

    #[test]
    fn rewrites_ref_assignment_with_unwrap_hint() {
        let original = "Type 'Ref<number>' is not assignable to type 'number'.";
        let rewritten = rewrite_corsa_message(original, "");
        assert!(rewritten.contains(original));
        assert!(rewritten.contains("Did you forget `.value`"));
    }

    #[test]
    fn rewrites_internal_vue_ts_imports_back_to_vue() {
        let original = "Cannot find module '../logo/MfMatesLogo.vue.ts' or its corresponding type declarations.";
        let rewritten = rewrite_corsa_message(original, "import '../logo/MfMatesLogo.vue';");

        assert_eq!(
            rewritten,
            "Cannot find module '../logo/MfMatesLogo.vue' or its corresponding type declarations."
        );
        assert!(!rewritten.contains(".vue.ts"));
    }

    #[test]
    fn rewrites_internal_vue_tsx_imports_back_to_vue() {
        let original =
            "Cannot find module './Panel.vue.tsx' or its corresponding type declarations.";
        let rewritten = rewrite_corsa_message(original, "import './Panel.vue';");

        assert_eq!(
            rewritten,
            "Cannot find module './Panel.vue' or its corresponding type declarations."
        );
        assert!(!rewritten.contains(".vue.tsx"));
    }

    #[test]
    fn rewrites_every_internal_vue_virtual_suffix_in_message() {
        let original = "Cannot find module '../logo/MfMatesLogo.vue.ts'. Related import './Panel.vue.tsx' also failed.";
        let rewritten = rewrite_corsa_message(original, "");

        assert_eq!(
            rewritten,
            "Cannot find module '../logo/MfMatesLogo.vue'. Related import './Panel.vue' also failed."
        );
        assert!(!rewritten.contains(".vue.ts"));
        assert!(!rewritten.contains(".vue.tsx"));
    }

    #[test]
    fn rewrites_internal_vue_suffix_before_adding_value_hint() {
        let original =
            "Property 'toFixed' does not exist on type 'typeof import(\"./Panel.vue.ts\")'.";
        let rewritten = rewrite_corsa_message(original, "");

        assert!(rewritten.contains("./Panel.vue"));
        assert!(!rewritten.contains(".vue.ts"));
        assert!(rewritten.contains(".value"));
    }

    #[test]
    fn passes_through_unrelated_messages() {
        let original = "Expected 1 argument, but got 0.";
        assert_eq!(rewrite_corsa_message(original, ""), original);
    }

    #[test]
    fn strips_private_corsa_overlay_roots_from_messages() {
        let original = "Could not find a declaration file for module '../docs/.vuepress/data/books'. '/workspace/node_modules/.vize/corsa/44018-2/overlays/workspace/docs/.vuepress/data/books.js' implicitly has an 'any' type.";

        assert_eq!(
            strip_corsa_overlay_paths(original),
            "Could not find a declaration file for module '../docs/.vuepress/data/books'. '/workspace/docs/.vuepress/data/books.js' implicitly has an 'any' type."
        );
    }

    #[test]
    fn strips_multiple_private_corsa_overlay_roots_from_messages() {
        let original = "Related files: /repo/node_modules/.vize/corsa/session/overlays/repo/a.ts and /repo/node_modules/.vize/corsa/session/overlays/repo/b.ts";

        assert_eq!(
            strip_corsa_overlay_paths(original),
            "Related files: /repo/a.ts and /repo/b.ts"
        );
    }

    #[test]
    fn property_extractor_returns_name() {
        assert_eq!(
            property_does_not_exist_property("Property 'foo' does not exist on type 'Bar'."),
            Some("foo")
        );
        assert_eq!(
            property_does_not_exist_property("Cannot find name 'foo'."),
            None
        );
    }

    #[test]
    fn preserves_authored_vue_ts_and_restores_collision_marker() {
        let marker = vize_canon::batch::AUTHORED_VUE_TS_SENTINEL;
        let original = cstr!("Cannot find module './Missing.vue.ts{marker}'.");
        assert_eq!(
            rewrite_corsa_message(&original, "import './Missing.vue.ts';"),
            "Cannot find module './Missing.vue.ts'."
        );

        let authored = "Cannot find module './Authored.vue.ts'.";
        assert_eq!(
            rewrite_corsa_message(authored, "import './Authored.vue.ts';"),
            authored
        );
    }
}
