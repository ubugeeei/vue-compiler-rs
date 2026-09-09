//! Text-level matching of a mutation target against the component's props.
//!
//! Callers pass the *source slice of an assignment target* (`props.msg`,
//! `user.name`, `count`), never a whole expression, so matching a prefix here
//! cannot pick up an unrelated occurrence elsewhere in the expression.

use vize_s0::FxHashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MutationTargetKind {
    Direct,
    Deep,
}

#[cfg(test)]
pub(super) fn is_prop_mutation_target(
    content: &str,
    prop_names: &FxHashSet<&str>,
    has_props_object_binding: bool,
) -> bool {
    prop_mutation_target_kind(content, prop_names, has_props_object_binding).is_some()
}

pub(super) fn prop_mutation_target_kind(
    content: &str,
    prop_names: &FxHashSet<&str>,
    has_props_object_binding: bool,
) -> Option<MutationTargetKind> {
    let content = content.trim();
    if prop_names.contains(content) {
        return Some(MutationTargetKind::Direct);
    }

    if has_props_object_binding
        && content
            .strip_prefix("props")
            .and_then(|rest| props_object_member_mutation_kind(rest, prop_names))
            .is_some()
    {
        return content
            .strip_prefix("props")
            .and_then(|rest| props_object_member_mutation_kind(rest, prop_names));
    }

    prop_names.iter().find_map(|name| {
        content
            .strip_prefix(*name)
            .and_then(|rest| is_member_access_suffix(rest).then_some(MutationTargetKind::Deep))
    })
}

fn is_member_access_suffix(rest: &str) -> bool {
    rest.starts_with('.') || rest.starts_with('[') || rest.starts_with("?.")
}

fn props_object_member_mutation_kind(
    rest: &str,
    prop_names: &FxHashSet<&str>,
) -> Option<MutationTargetKind> {
    if let Some((name, suffix)) = props_member_root(rest) {
        if prop_names.is_empty() || prop_names.contains(name) {
            return Some(member_suffix_kind(suffix));
        }
        return None;
    }

    dynamic_props_member_access_kind(rest)
}

fn dynamic_props_member_access_kind(rest: &str) -> Option<MutationTargetKind> {
    let mut rest = rest.trim_start();
    if let Some(after_optional) = rest.strip_prefix("?.") {
        rest = after_optional.trim_start();
    }

    let after_bracket = rest.strip_prefix('[')?;
    let after_bracket = after_bracket.trim_start();
    if after_bracket.starts_with('\'') || after_bracket.starts_with('"') {
        return None;
    }
    let close = after_bracket.find(']')?;
    Some(member_suffix_kind(&after_bracket[close + 1..]))
}

fn props_member_root(rest: &str) -> Option<(&str, &str)> {
    let mut rest = rest.trim_start();
    let mut consumed_optional = false;
    if let Some(after_optional) = rest.strip_prefix("?.") {
        rest = after_optional.trim_start();
        consumed_optional = true;
    }

    if let Some(after_dot) = rest.strip_prefix('.') {
        return identifier_root(after_dot);
    }

    if consumed_optional && let Some(root) = identifier_root(rest) {
        return Some(root);
    }

    let after_bracket = rest.strip_prefix('[')?.trim_start();
    let quote = after_bracket.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let name_start = quote.len_utf8();
    let name_end = after_bracket[name_start..].find(quote)? + name_start;
    let after_quote = &after_bracket[name_end + quote.len_utf8()..];
    let after_close = after_quote.strip_prefix(']')?;
    (name_end > name_start).then_some((&after_bracket[name_start..name_end], after_close))
}

fn identifier_root(source: &str) -> Option<(&str, &str)> {
    let end = source
        .find(|ch: char| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        .unwrap_or(source.len());
    (end > 0).then_some((&source[..end], &source[end..]))
}

fn member_suffix_kind(suffix: &str) -> MutationTargetKind {
    if is_member_access_suffix(suffix.trim_start()) {
        MutationTargetKind::Deep
    } else {
        MutationTargetKind::Direct
    }
}

#[cfg(test)]
mod tests {
    use super::{MutationTargetKind, is_prop_mutation_target, prop_mutation_target_kind};
    use vize_s0::FxHashSet;

    #[test]
    fn prop_mutation_target_matches_member_roots() {
        let prop_names = FxHashSet::from_iter(["count", "user"]);

        assert!(is_prop_mutation_target("count", &prop_names, false));
        assert!(is_prop_mutation_target("user.name", &prop_names, false));
        assert!(is_prop_mutation_target("user?.name", &prop_names, false));
        assert!(is_prop_mutation_target("props.count", &prop_names, true));
        assert!(is_prop_mutation_target(
            "props.user.name",
            &prop_names,
            true
        ));
        assert!(is_prop_mutation_target("props['count']", &prop_names, true));
        assert!(is_prop_mutation_target("props[key]", &prop_names, true));
        assert!(is_prop_mutation_target(
            "props[key].name",
            &prop_names,
            true
        ));
        assert!(is_prop_mutation_target(
            "props?.user.name",
            &prop_names,
            true
        ));
        assert!(!is_prop_mutation_target("props.extra", &prop_names, true));
        assert!(!is_prop_mutation_target(
            "props['extra']",
            &prop_names,
            true
        ));
        assert!(!is_prop_mutation_target(
            "props.user.name",
            &prop_names,
            false
        ));
        assert!(!is_prop_mutation_target(
            "counter.value",
            &prop_names,
            false
        ));
        assert!(!is_prop_mutation_target(
            "propsState.count",
            &prop_names,
            true
        ));

        let unknown_prop_names = FxHashSet::default();
        assert!(is_prop_mutation_target(
            "props.title",
            &unknown_prop_names,
            true
        ));
        assert!(is_prop_mutation_target(
            "props[field]",
            &unknown_prop_names,
            true
        ));
    }

    #[test]
    fn prop_mutation_target_kind_distinguishes_direct_and_deep() {
        let prop_names = FxHashSet::from_iter(["count", "user"]);

        assert_eq!(
            prop_mutation_target_kind("count", &prop_names, false),
            Some(MutationTargetKind::Direct)
        );
        assert_eq!(
            prop_mutation_target_kind("user.name", &prop_names, false),
            Some(MutationTargetKind::Deep)
        );
        assert_eq!(
            prop_mutation_target_kind("props.count", &prop_names, true),
            Some(MutationTargetKind::Direct)
        );
        assert_eq!(
            prop_mutation_target_kind("props.user.name", &prop_names, true),
            Some(MutationTargetKind::Deep)
        );
        assert_eq!(
            prop_mutation_target_kind("props['count']", &prop_names, true),
            Some(MutationTargetKind::Direct)
        );
        assert_eq!(
            prop_mutation_target_kind("props['user'].name", &prop_names, true),
            Some(MutationTargetKind::Deep)
        );
        assert_eq!(
            prop_mutation_target_kind("props[key]", &prop_names, true),
            Some(MutationTargetKind::Direct)
        );
        assert_eq!(
            prop_mutation_target_kind("props[key].name", &prop_names, true),
            Some(MutationTargetKind::Deep)
        );
    }
}
