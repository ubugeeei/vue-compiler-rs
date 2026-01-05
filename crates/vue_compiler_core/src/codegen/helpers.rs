//! Utility functions for code generation.

use crate::ast::RuntimeHelper;

/// Escape a string for use in JavaScript string literals
pub fn escape_js_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\x08' => result.push_str("\\b"), // backspace
            '\x0C' => result.push_str("\\f"), // form feed
            c if c.is_control() => {
                // Other control characters as unicode escape
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Check if a string is a valid JavaScript identifier (doesn't need quoting)
pub fn is_valid_js_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    // First character must be a letter, underscore, or dollar sign
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    // Remaining characters can also include digits
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Default helper alias function
pub fn default_helper_alias(helper: RuntimeHelper) -> &'static str {
    match helper {
        RuntimeHelper::Fragment => "_Fragment",
        RuntimeHelper::Teleport => "_Teleport",
        RuntimeHelper::Suspense => "_Suspense",
        RuntimeHelper::KeepAlive => "_KeepAlive",
        RuntimeHelper::BaseTransition => "_BaseTransition",
        RuntimeHelper::Transition => "_Transition",
        RuntimeHelper::TransitionGroup => "_TransitionGroup",
        RuntimeHelper::OpenBlock => "_openBlock",
        RuntimeHelper::CreateBlock => "_createBlock",
        RuntimeHelper::CreateElementBlock => "_createElementBlock",
        RuntimeHelper::CreateVNode => "_createVNode",
        RuntimeHelper::CreateElementVNode => "_createElementVNode",
        RuntimeHelper::CreateComment => "_createCommentVNode",
        RuntimeHelper::CreateText => "_createTextVNode",
        RuntimeHelper::CreateStatic => "_createStaticVNode",
        RuntimeHelper::ResolveComponent => "_resolveComponent",
        RuntimeHelper::ResolveDynamicComponent => "_resolveDynamicComponent",
        RuntimeHelper::ResolveDirective => "_resolveDirective",
        RuntimeHelper::ResolveFilter => "_resolveFilter",
        RuntimeHelper::WithDirectives => "_withDirectives",
        RuntimeHelper::VShow => "_vShow",
        RuntimeHelper::VModelText => "_vModelText",
        RuntimeHelper::VModelCheckbox => "_vModelCheckbox",
        RuntimeHelper::VModelRadio => "_vModelRadio",
        RuntimeHelper::VModelSelect => "_vModelSelect",
        RuntimeHelper::VModelDynamic => "_vModelDynamic",
        RuntimeHelper::RenderList => "_renderList",
        RuntimeHelper::RenderSlot => "_renderSlot",
        RuntimeHelper::CreateSlots => "_createSlots",
        RuntimeHelper::ToDisplayString => "_toDisplayString",
        RuntimeHelper::MergeProps => "_mergeProps",
        RuntimeHelper::NormalizeClass => "_normalizeClass",
        RuntimeHelper::NormalizeStyle => "_normalizeStyle",
        RuntimeHelper::NormalizeProps => "_normalizeProps",
        RuntimeHelper::GuardReactiveProps => "_guardReactiveProps",
        RuntimeHelper::ToHandlers => "_toHandlers",
        RuntimeHelper::Camelize => "_camelize",
        RuntimeHelper::Capitalize => "_capitalize",
        RuntimeHelper::ToHandlerKey => "_toHandlerKey",
        RuntimeHelper::SetBlockTracking => "_setBlockTracking",
        RuntimeHelper::PushScopeId => "_pushScopeId",
        RuntimeHelper::PopScopeId => "_popScopeId",
        RuntimeHelper::WithCtx => "_withCtx",
        RuntimeHelper::Unref => "_unref",
        RuntimeHelper::IsRef => "_isRef",
        RuntimeHelper::WithMemo => "_withMemo",
        RuntimeHelper::IsMemoSame => "_isMemoSame",
        RuntimeHelper::WithModifiers => "_withModifiers",
        RuntimeHelper::WithKeys => "_withKeys",
    }
}

/// Capitalize first letter of a string
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "".into(),
    }
}

/// Convert kebab-case to camelCase
pub fn camelize(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;

    for c in s.chars() {
        if c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

/// Check if a component is a Vue built-in that should be imported directly
pub fn is_builtin_component(name: &str) -> Option<RuntimeHelper> {
    match name {
        "Teleport" => Some(RuntimeHelper::Teleport),
        "Suspense" => Some(RuntimeHelper::Suspense),
        "KeepAlive" => Some(RuntimeHelper::KeepAlive),
        "BaseTransition" => Some(RuntimeHelper::BaseTransition),
        "Transition" => Some(RuntimeHelper::Transition),
        "TransitionGroup" => Some(RuntimeHelper::TransitionGroup),
        _ => None,
    }
}
