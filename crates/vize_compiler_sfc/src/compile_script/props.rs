//! Props and emit type extraction utilities.
//!
//! This module handles extracting prop types from TypeScript type definitions
//! and processing withDefaults defaults.

use std::collections::HashMap;

/// Prop type information
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PropTypeInfo {
    /// JavaScript type constructor name (String, Number, Boolean, Array, Object, Function)
    pub js_type: String,
    /// Original TypeScript type (for PropType<T> usage)
    pub ts_type: Option<String>,
    /// Whether the prop is optional
    pub optional: bool,
}

/// Extract prop types from TypeScript type definition
pub fn extract_prop_types_from_type(type_args: &str) -> HashMap<String, PropTypeInfo> {
    let mut props = HashMap::new();

    let content = type_args.trim();
    let content = if content.starts_with('{') && content.ends_with('}') {
        &content[1..content.len() - 1]
    } else {
        content
    };

    // Split by commas/semicolons/newlines (but not inside nested braces)
    let mut depth = 0;
    let mut current = String::new();

    for c in content.chars() {
        match c {
            '{' | '<' | '(' | '[' => {
                depth += 1;
                current.push(c);
            }
            '}' | '>' | ')' | ']' => {
                depth -= 1;
                current.push(c);
            }
            ',' | ';' | '\n' if depth == 0 => {
                extract_prop_type_info(&current, &mut props);
                current.clear();
            }
            _ => current.push(c),
        }
    }
    extract_prop_type_info(&current, &mut props);

    props
}

fn extract_prop_type_info(segment: &str, props: &mut HashMap<String, PropTypeInfo>) {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return;
    }

    // Parse "name?: Type" or "name: Type"
    if let Some(colon_pos) = trimmed.find(':') {
        let name_part = &trimmed[..colon_pos];
        let type_part = &trimmed[colon_pos + 1..];

        let optional = name_part.ends_with('?');
        let name = name_part.trim().trim_end_matches('?').trim();

        if !name.is_empty() && is_valid_identifier(name) {
            let ts_type_str = type_part.trim().to_string();
            let js_type = ts_type_to_js_type(&ts_type_str);
            props.insert(
                name.to_string(),
                PropTypeInfo {
                    js_type,
                    ts_type: Some(ts_type_str),
                    optional,
                },
            );
        }
    }
}

/// Convert TypeScript type to JavaScript type constructor
fn ts_type_to_js_type(ts_type: &str) -> String {
    let ts_type = ts_type.trim();

    // Handle string literal types: "foo" or 'bar' -> String
    if (ts_type.starts_with('"') && ts_type.ends_with('"'))
        || (ts_type.starts_with('\'') && ts_type.ends_with('\''))
    {
        return "String".to_string();
    }

    // Handle numeric literal types: 123, 1.5 -> Number
    if ts_type.parse::<f64>().is_ok() {
        return "Number".to_string();
    }

    // Handle boolean literal types: true, false -> Boolean
    if ts_type == "true" || ts_type == "false" {
        return "Boolean".to_string();
    }

    // Handle union types - take the first non-undefined/null type
    if ts_type.contains('|') {
        let parts: Vec<&str> = ts_type.split('|').collect();
        for part in parts {
            let part = part.trim();
            if part != "undefined" && part != "null" {
                return ts_type_to_js_type(part);
            }
        }
    }

    // Map TypeScript types to JavaScript constructors
    match ts_type.to_lowercase().as_str() {
        "string" => "String".to_string(),
        "number" => "Number".to_string(),
        "boolean" => "Boolean".to_string(),
        "object" => "Object".to_string(),
        "function" => "Function".to_string(),
        "symbol" => "Symbol".to_string(),
        _ => {
            // Handle array types
            if ts_type.ends_with("[]") || ts_type.starts_with("Array<") {
                "Array".to_string()
            } else if ts_type.starts_with('{') || ts_type.contains(':') {
                // Object literal type
                "Object".to_string()
            } else if ts_type.starts_with('(') && ts_type.contains("=>") {
                // Function type
                "Function".to_string()
            } else {
                // Check if this is a built-in JavaScript constructor type
                let type_name = ts_type.split('<').next().unwrap_or(ts_type).trim();
                match type_name {
                    // Built-in JavaScript types that exist at runtime
                    "Date" | "RegExp" | "Error" | "Map" | "Set" | "WeakMap" | "WeakSet"
                    | "Promise" | "ArrayBuffer" | "DataView" | "Int8Array" | "Uint8Array"
                    | "Int16Array" | "Uint16Array" | "Int32Array" | "Uint32Array"
                    | "Float32Array" | "Float64Array" | "BigInt64Array" | "BigUint64Array"
                    | "URL" | "URLSearchParams" | "FormData" | "Blob" | "File" => {
                        type_name.to_string()
                    }
                    // User-defined interface/type - use Object (types don't exist at runtime)
                    _ => "Object".to_string(),
                }
            }
        }
    }
}

/// Extract emit names from TypeScript type definition
pub fn extract_emit_names_from_type(type_args: &str) -> Vec<String> {
    let mut emits = Vec::new();

    // Match patterns like: (e: 'eventName') or (event: 'eventName', ...)
    let mut in_string = false;
    let mut quote_char = ' ';
    let mut current_string = String::new();

    for c in type_args.chars() {
        if !in_string && (c == '\'' || c == '"') {
            in_string = true;
            quote_char = c;
            current_string.clear();
        } else if in_string && c == quote_char {
            in_string = false;
            if !current_string.is_empty() {
                emits.push(current_string.clone());
            }
        } else if in_string {
            current_string.push(c);
        }
    }

    emits
}

/// Extract default values from withDefaults second argument
/// Input: "withDefaults(defineProps<{...}>(), { prop1: default1, prop2: default2 })"
/// Returns: HashMap of prop name to default value string
pub fn extract_with_defaults_defaults(with_defaults_args: &str) -> HashMap<String, String> {
    let mut defaults = HashMap::new();

    // Find the second argument (the defaults object)
    // withDefaults(defineProps<...>(), { ... })
    // We need to find the { after "defineProps<...>()"

    let content = with_defaults_args.trim();
    let chars: Vec<char> = content.chars().collect();

    // First, find "defineProps" and then its closing parenthesis
    let define_props_pos = content.find("defineProps");
    if define_props_pos.is_none() {
        return defaults;
    }

    let start_search = define_props_pos.unwrap();
    let mut paren_depth = 0;
    let mut in_define_props_call = false;
    let mut found_define_props_end = false;
    let mut defaults_start = None;

    let mut i = start_search;
    while i < chars.len() {
        let c = chars[i];

        if !in_define_props_call {
            // Looking for the opening paren of defineProps()
            if c == '(' {
                in_define_props_call = true;
                paren_depth = 1;
            }
        } else if !found_define_props_end {
            match c {
                '(' => paren_depth += 1,
                ')' => {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        found_define_props_end = true;
                    }
                }
                _ => {}
            }
        } else {
            // Looking for the defaults object start
            if c == '{' {
                defaults_start = Some(i);
                break;
            }
        }
        i += 1;
    }

    if let Some(start) = defaults_start {
        // Find matching closing brace
        let mut brace_depth = 0;
        let mut end = start;

        for (j, &c) in chars.iter().enumerate().skip(start) {
            match c {
                '{' => brace_depth += 1,
                '}' => {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        end = j;
                        break;
                    }
                }
                _ => {}
            }
        }

        // Extract the defaults object content (without braces)
        let defaults_content: String = chars[start + 1..end].iter().collect();
        parse_defaults_object(&defaults_content, &mut defaults);
    }

    defaults
}

/// Parse a JavaScript object literal to extract key-value pairs
fn parse_defaults_object(content: &str, defaults: &mut HashMap<String, String>) {
    let content = content.trim();
    if content.is_empty() {
        return;
    }

    // Split by commas, but respect nested braces/parens/brackets
    let mut depth = 0;
    let mut current = String::new();

    for c in content.chars() {
        match c {
            '{' | '(' | '[' => {
                depth += 1;
                current.push(c);
            }
            '}' | ')' | ']' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                extract_default_pair(&current, defaults);
                current.clear();
            }
            _ => current.push(c),
        }
    }
    extract_default_pair(&current, defaults);
}

/// Extract a single key: value pair from a default definition
fn extract_default_pair(pair: &str, defaults: &mut HashMap<String, String>) {
    let trimmed = pair.trim();
    if trimmed.is_empty() {
        return;
    }

    // Find the first : that's not inside a nested structure
    let mut depth = 0;
    let mut colon_pos = None;

    for (i, c) in trimmed.chars().enumerate() {
        match c {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth -= 1,
            ':' if depth == 0 => {
                colon_pos = Some(i);
                break;
            }
            _ => {}
        }
    }

    if let Some(pos) = colon_pos {
        let key = trimmed[..pos].trim();
        let value = trimmed[pos + 1..].trim();

        if !key.is_empty() && !value.is_empty() {
            defaults.insert(key.to_string(), value.to_string());
        }
    }
}

/// Check if a string is a valid JS identifier
pub fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }

    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}
