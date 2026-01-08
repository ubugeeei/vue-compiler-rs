//! Macro detection helpers.
//!
//! This module provides utilities for detecting Vue compiler macro calls
//! in script setup code.

/// Check if a line is a compiler macro call
pub fn is_macro_call_line(line: &str) -> bool {
    let macros = [
        "defineProps",
        "defineEmits",
        "defineExpose",
        "defineOptions",
        "defineSlots",
        "defineModel",
        "withDefaults",
    ];

    // Check if line contains a macro that is being called (not just imported)
    for macro_name in macros {
        if line.contains(macro_name) && line.contains('(') {
            // Make sure it's not an import
            if !line.trim().starts_with("import") {
                return true;
            }
        }
    }
    false
}

/// Check if a line starts a multi-line paren-based macro call (e.g., defineExpose({)
pub fn is_paren_macro_start(line: &str) -> bool {
    let macros = [
        "defineProps",
        "defineEmits",
        "defineExpose",
        "defineOptions",
        "defineSlots",
        "defineModel",
        "withDefaults",
    ];

    // Check if line contains a macro call that isn't complete on the same line
    for macro_name in macros {
        if line.contains(macro_name) && !line.trim().starts_with("import") {
            // Check for unbalanced parentheses (call spans multiple lines)
            if line.contains('(') {
                let open_count = line.matches('(').count();
                let close_count = line.matches(')').count();
                if open_count > close_count {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if a line starts a multi-line macro call (e.g., defineEmits<{ ... }>())
pub fn is_multiline_macro_start(line: &str) -> bool {
    let macros = [
        "defineProps",
        "defineEmits",
        "defineExpose",
        "defineOptions",
        "defineSlots",
        "defineModel",
        "withDefaults",
    ];

    // Check if line contains a macro with type args that spans multiple lines
    // Pattern: contains macro name, contains '<', but doesn't have matching '>' on same line
    // or has '>' but no '()' yet
    for macro_name in macros {
        if line.contains(macro_name) && !line.trim().starts_with("import") {
            // Check for type args that might span multiple lines
            if line.contains('<') {
                let open_count = line.matches('<').count();
                let close_count = line.matches('>').count();
                // If angle brackets aren't balanced, it's multi-line
                if open_count > close_count {
                    return true;
                }
                // If balanced but no () at the end, might still be multi-line
                if open_count == close_count && !line.contains("()") && !line.ends_with(')') {
                    // Check if this is a complete single-line call
                    // e.g., defineEmits<(e: 'click') => void>() - this has ()
                    // vs defineEmits<{ - this doesn't have () yet
                    if !line.trim().ends_with("()") && !line.trim().ends_with(')') {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if a line is a props destructure pattern
pub fn is_props_destructure_line(line: &str) -> bool {
    let trimmed = line.trim();
    // Match: const { ... } = defineProps or const { ... } = withDefaults
    (trimmed.starts_with("const {") || trimmed.starts_with("let {") || trimmed.starts_with("var {"))
        && (trimmed.contains("defineProps") || trimmed.contains("withDefaults"))
}
