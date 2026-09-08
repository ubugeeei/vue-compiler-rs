//! Helper functions and constants for virtual TypeScript generation.
//!
//! Contains utility functions for type declarations, event type mapping,
//! identifier conversion, and template context generation.

use std::ops::Range;

mod dom_events;
mod preamble;
mod vue2_members;

pub(crate) use dom_events::{get_dom_event_type, is_known_dom_event_name};
pub use preamble::{
    DECLARATION_HELPERS_DTS, SHARED_PREAMBLE_DTS, SHARED_PREAMBLE_FILE_NAME, VUE_SETUP_HELPERS,
    VUE_TYPE_HELPERS,
};
pub(crate) use preamble::{
    EMIT_OVERLOAD_HELPERS, EMIT_PROPS_HELPER, SETUP_SCOPE_HELPER_NAMES, VUE_SETUP_HELPERS_HOISTED,
};
use vue2_members::VUE2_INSTANCE_MEMBERS;
pub(crate) use vue2_members::is_vue2_instance_member;

use super::types::VirtualTsOptions;
use vize_carton::config::VueVersion;
use vize_carton::{String, append};

/// Generate Vue template context declarations dynamically.
///
/// Uses Vue's `ComponentPublicInstance` for Vue 3. In legacy Vue 2 mode, emits
/// a structural fallback because Vue 2.6 does not export that Vue 3 helper type.
pub(crate) fn generate_template_context(
    options: &VirtualTsOptions,
    dialect: VueVersion,
    legacy_vue2: bool,
) -> String {
    let mut ctx = String::default();

    let needs_global_helper =
        !options.template_globals.is_empty() || !options.css_modules.is_empty();

    // Instance type + conditional accessor helper
    let vue2_dialect = legacy_vue2 || matches!(dialect, VueVersion::V2 | VueVersion::V2_7);
    if vue2_dialect {
        ctx.push_str("    // Vue template context (Vue 2-compatible structural fallback)\n    type __Ctx = { $attrs: Record<string, unknown>; $slots: Record<string, unknown>; $refs: Record<string, any>; $emit: (...args: any[]) => void; };\n");
    } else {
        ctx.push_str("    // Vue template context (delegates to ComponentPublicInstance)\n    // @ts-ignore TS2694/TS2307: a `vue` without `ComponentPublicInstance` must degrade template context extras to unchecked, never error.\n    type __Ctx = import('vue').ComponentPublicInstance;\n");
    }
    if needs_global_helper {
        ctx.push_str("    type __VizeGlobalContextIsAny<T> = 0 extends (1 & T) ? true : false;\n    type __VizeGlobalContextProperty<T, F> = __VizeGlobalContextIsAny<T> extends true ? T : unknown extends T ? F : T;\n    type __Global<K extends string, F = unknown> = K extends keyof __Ctx ? __VizeGlobalContextProperty<__Ctx[K], F> : F;\n");
    }
    ctx.push_str("    const __ctx = undefined as unknown as __Ctx;\n");
    if options.strict_instance_globals {
        ctx.push_str(
            "    type __VizeStrictContextIsAny<T> = 0 extends (1 & T) ? true : false;\n    type __VizeStrictContextLiteralKey<T> = T extends string ? string extends T ? never : T extends `$${infer __Rest}` ? string extends __Rest ? never : T : T : number extends T ? never : symbol extends T ? never : T;\n    type __VizeStrictContextProperty<T> = [T] extends [never] ? never : __VizeStrictContextIsAny<T> extends true ? T : unknown extends T ? any : T;\n    type __VizeStrictPublicInstanceGlobals = {\n      [__K in keyof __Ctx as __VizeStrictContextLiteralKey<__K> extends infer __L ? __L extends `$${string}` ? __L : never : never]: __VizeStrictContextProperty<__Ctx[__K]>;\n    };\n    type __VizeStrictTemplateContext = {\n      $attrs: __Ctx[\"$attrs\"];\n      $slots: __Ctx[\"$slots\"];\n      $refs: __Ctx[\"$refs\"];\n      $emit: __Ctx[\"$emit\"];\n    } & __VizeStrictPublicInstanceGlobals;\n    const __vize_strict_template_context = undefined as unknown as __VizeStrictTemplateContext;\n",
        );
    }

    // Core Vue globals (always present on ComponentPublicInstance)
    ctx.push_str("    const $attrs = __ctx.$attrs;\n");
    ctx.push_str("    const $slots = __ctx.$slots;\n");
    ctx.push_str("    const $refs = __ctx.$refs;\n");
    ctx.push_str("    const $emit = __ctx.$emit;\n");

    // Vue 2-only instance members (absent from Vue 3's ComponentPublicInstance).
    if vue2_dialect {
        ctx.push_str("    // Vue 2 instance members (not on Vue 3 ComponentPublicInstance)\n");
        for member in VUE2_INSTANCE_MEMBERS {
            append!(ctx, "    const {member} = undefined as any;\n");
        }
    }

    // Plugin globals (resolved via ComponentCustomProperties if augmented,
    // otherwise falls back to the configured type_annotation)
    if !options.template_globals.is_empty() {
        ctx.push_str("    // Plugin globals (via ComponentCustomProperties)\n");
        for global in &options.template_globals {
            let type_annotation = global.context_type_annotation();
            append!(
                ctx,
                "    const {}: {} = undefined as any;\n",
                global.name,
                type_annotation
            );
        }
    }

    // CSS module globals (resolved via ComponentCustomProperties if augmented,
    // otherwise falls back to Record<string, string>)
    if !options.css_modules.is_empty() {
        ctx.push_str("    // CSS modules (from <style module>)\n");
        for module_name in &options.css_modules {
            append!(
                ctx,
                "    const {module_name}: __Global<'{module_name}', Record<string, string>> = undefined as any;\n"
            );
        }
    }

    // Mark all as used
    ctx.push_str("    void __ctx; void $attrs; void $slots; void $refs; void $emit;");
    if options.strict_instance_globals {
        ctx.push_str(" void __vize_strict_template_context;");
    }
    ctx.push('\n');
    if vue2_dialect {
        ctx.push_str("    ");
        for member in VUE2_INSTANCE_MEMBERS {
            append!(ctx, "void {member};");
        }
        ctx.push('\n');
    }
    if !options.template_globals.is_empty() {
        ctx.push_str("    ");
        for (i, global) in options.template_globals.iter().enumerate() {
            if i > 0 {
                ctx.push(' ');
            }
            append!(ctx, "void {};", global.name);
        }
        ctx.push('\n');
    }
    if !options.css_modules.is_empty() {
        ctx.push_str("    ");
        for (i, module_name) in options.css_modules.iter().enumerate() {
            if i > 0 {
                ctx.push(' ');
            }
            append!(ctx, "void {module_name};");
        }
        ctx.push('\n');
    }

    ctx
}

/// Get the generated subrange that corresponds to a specific source expression.
///
/// This keeps source maps anchored to the actual expression text instead of
/// any wrapping code we emit around it (`void (...)`, `as Foo`, handler shims).
pub(crate) fn generated_text_range(
    generated_segment: &str,
    mapped_text: &str,
    generated_start: usize,
) -> Range<usize> {
    if mapped_text.is_empty() {
        return generated_start..generated_start + generated_segment.len();
    }

    let relative_start = generated_segment.find(mapped_text).unwrap_or(0);
    let start = generated_start + relative_start;
    start..start + mapped_text.len()
}

/// Resolve a template attribute name to the prop key Vue matches it against.
///
/// Mirrors Vue's `camelize` (`str.replace(/-(\w)/g, …)`) exactly: a hyphen is
/// dropped and the character after it uppercased, nothing else changes. The
/// leading character keeps its case and `_` is not a separator; renaming either
/// way looks up a key no declared prop matches, so a bound prop reads as missing
/// (#3863). `"my-prop"` -> `"myProp"`, `"MyProp"` -> `"MyProp"`, `"my_prop"` -> `"my_prop"`.
pub(crate) fn to_camel_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;

    for c in s.chars() {
        if c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

/// Sanitize a string to be a valid TypeScript identifier.
/// Replaces invalid characters (like ':') with underscores and prefixes
/// reserved words.
/// Examples: "update:title" -> "update_title", "my-event" -> "my_event"
pub(crate) fn to_safe_identifier(s: &str) -> String {
    let mut result = to_safe_identifier_fragment(s);

    if !result
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
    {
        result.insert(0, '_');
    }
    if is_reserved_identifier(result.as_str()) {
        result.insert(0, '_');
    }

    result
}

/// Sanitize a string for use inside a generated identifier that already has a
/// safe prefix (for example `_slot_{name}`).
pub(crate) fn to_safe_identifier_fragment(s: &str) -> String {
    let mut result = String::with_capacity(s.len().max(1));

    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            result.push(c);
        } else {
            result.push('_');
        }
    }

    if result.is_empty() {
        result.push('_');
    }

    result
}

#[inline]
pub(crate) fn is_reserved_identifier(s: &str) -> bool {
    matches!(
        s,
        "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "let"
            | "static"
            | "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "as"
            | "from"
            | "of"
    )
}
