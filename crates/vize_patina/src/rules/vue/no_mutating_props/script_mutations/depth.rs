#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::rules::vue::no_mutating_props) enum ScriptPropMutationKind {
    Direct,
    Deep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PropBindingKind {
    Object,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MutationOperation {
    Assign,
    MutatingCall,
}

pub(super) fn script_mutation_kind(
    root_name: &str,
    target: &str,
    binding_kind: PropBindingKind,
    operation: MutationOperation,
) -> ScriptPropMutationKind {
    let target = target.trim();
    let suffix = target
        .strip_prefix(root_name)
        .map(str::trim_start)
        .unwrap_or_default();

    match (binding_kind, operation) {
        (PropBindingKind::Value, MutationOperation::MutatingCall) => ScriptPropMutationKind::Deep,
        (PropBindingKind::Value, MutationOperation::Assign) if suffix.is_empty() => {
            ScriptPropMutationKind::Direct
        }
        (PropBindingKind::Value, MutationOperation::Assign) => ScriptPropMutationKind::Deep,
        (PropBindingKind::Object, MutationOperation::MutatingCall) if suffix.is_empty() => {
            ScriptPropMutationKind::Direct
        }
        (PropBindingKind::Object, MutationOperation::MutatingCall) => ScriptPropMutationKind::Deep,
        (PropBindingKind::Object, MutationOperation::Assign) => {
            props_object_script_mutation_kind(suffix)
        }
    }
}

fn props_object_script_mutation_kind(suffix: &str) -> ScriptPropMutationKind {
    if member_access_count(suffix) > 1 {
        ScriptPropMutationKind::Deep
    } else {
        ScriptPropMutationKind::Direct
    }
}

fn member_access_count(mut rest: &str) -> u8 {
    let mut count = 0;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return count;
        }

        let after_member = if let Some(after_optional) = rest.strip_prefix("?.") {
            consume_member(after_optional)
        } else if let Some(after_dot) = rest.strip_prefix('.') {
            consume_identifier(after_dot)
        } else if let Some(after_bracket) = rest.strip_prefix('[') {
            consume_bracket(after_bracket)
        } else {
            None
        };

        let Some(after_member) = after_member else {
            return count;
        };
        count += 1;
        if count > 1 {
            return count;
        }
        rest = after_member;
    }
}

fn consume_member(rest: &str) -> Option<&str> {
    if let Some(after_bracket) = rest.strip_prefix('[') {
        consume_bracket(after_bracket)
    } else {
        consume_identifier(rest)
    }
}

fn consume_identifier(source: &str) -> Option<&str> {
    let end = source
        .find(|ch: char| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        .unwrap_or(source.len());
    (end > 0).then_some(&source[end..])
}

fn consume_bracket(source: &str) -> Option<&str> {
    let close = source.find(']')?;
    Some(&source[close + 1..])
}
