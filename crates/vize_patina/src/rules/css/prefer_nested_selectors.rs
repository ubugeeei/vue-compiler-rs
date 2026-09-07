//! css/prefer-nested-selectors
//!
//! Recommend using CSS nesting for descendant selectors.

use lightningcss::stylesheet::StyleSheet;

use crate::diagnostic::{LintDiagnostic, Severity};

use super::{CssLintResult, CssRule, CssRuleMeta};

static META: CssRuleMeta = CssRuleMeta {
    name: "css/prefer-nested-selectors",
    description: "Recommend using CSS nesting for descendant selectors",
    default_severity: Severity::Warning,
};

/// Prefer nested selectors rule
pub struct PreferNestedSelectors;

impl CssRule for PreferNestedSelectors {
    fn meta(&self) -> &'static CssRuleMeta {
        &META
    }

    fn check<'i>(
        &self,
        source: &'i str,
        _stylesheet: &StyleSheet<'i>,
        offset: usize,
        result: &mut CssLintResult,
    ) {
        scan(source, offset, result);
    }
}

/// At-rules whose body holds no style rules, so the whole block is skipped.
const NON_NESTED_BLOCK_AT_RULES: &[&str] = &[
    "keyframes",
    "-webkit-keyframes",
    "-moz-keyframes",
    "font-face",
    "page",
    "counter-style",
    "property",
    "font-feature-values",
    "color-profile",
    "viewport",
];

/// What an open brace opened.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A style rule. Everything inside it is *already nested*, so a
    /// descendant selector there is not worth reporting — there is
    /// nothing further to nest it into.
    Style,
    /// A conditional group at-rule (`@media`, `@supports`, `@container`,
    /// `@layer`). Its body sits at the nesting level of the at-rule
    /// itself, so a descendant selector inside one still reports.
    Group,
}

/// Walk the stylesheet's braces, reporting a descendant selector only
/// where nesting it would be an improvement.
///
/// The scan keeps a prelude running from the last boundary (`{`, `}` or
/// `;`) to the next `{`. Treating a declaration as a boundary is what
/// separates a selector from the text before it: without that,
/// `.a { color: red; .b {} }` reads `color: red;\n\n  .b` as one
/// selector, finds a space in it, and reports the nesting it is meant to
/// be recommending. Tracking [`Frame::Style`] is the other half — inside
/// a style rule the author has already nested.
fn scan(source: &str, offset: usize, result: &mut CssLintResult) {
    let bytes = source.as_bytes();
    let mut frames: Vec<Frame> = Vec::new();
    let mut prelude_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = skip_comment(bytes, i);
                continue;
            }
            b'"' | b'\'' => {
                i = skip_string(bytes, i);
                continue;
            }
            b';' => {
                i += 1;
                prelude_start = i;
            }
            b'}' => {
                frames.pop();
                i += 1;
                prelude_start = i;
            }
            b'{' => {
                let prelude = source[prelude_start..i].trim();
                if let Some(keyword) = at_keyword(prelude) {
                    if is_opaque_at_rule(keyword) {
                        i = skip_balanced_block(bytes, i);
                        prelude_start = i;
                        continue;
                    }
                    frames.push(Frame::Group);
                } else {
                    if !frames.contains(&Frame::Style)
                        && !prelude.is_empty()
                        && !is_already_nested(prelude)
                        && split_descendant_selector(prelude).is_some()
                    {
                        // Point at the selector, not the whitespace
                        // that separated it from the previous rule.
                        let lead = source[prelude_start..i].len()
                            - source[prelude_start..i].trim_start().len();
                        report(
                            prelude_start + lead,
                            prelude_start + lead + prelude.len(),
                            offset,
                            result,
                        );
                    }
                    frames.push(Frame::Style);
                }
                i += 1;
                prelude_start = i;
            }
            _ => i += 1,
        }
    }
}

fn report(start: usize, end: usize, offset: usize, result: &mut CssLintResult) {
    result.add_diagnostic(
        LintDiagnostic::warn(
            META.name,
            "Consider using CSS nesting for descendant selectors",
            u32::try_from(offset + start).unwrap_or(u32::MAX),
            u32::try_from(offset + end).unwrap_or(u32::MAX),
        )
        .with_help("Use CSS nesting syntax to nest child selectors inside parent selectors"),
    );
}

fn skip_comment(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return i + 2;
        }
        i += 1;
    }
    bytes.len()
}

fn skip_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            byte if byte == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// The at-rule keyword a prelude opens with, or `None` when it is a
/// selector.
fn at_keyword(prelude: &str) -> Option<&str> {
    let rest = prelude.strip_prefix('@')?;
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

fn is_opaque_at_rule(keyword: &str) -> bool {
    NON_NESTED_BLOCK_AT_RULES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(keyword))
}

fn skip_balanced_block(bytes: &[u8], open_pos: usize) -> usize {
    let mut depth: i32 = 0;
    let mut i = open_pos;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

fn is_already_nested(selector: &str) -> bool {
    let bytes = selector.as_bytes();
    let (mut bracket, mut paren) = (0usize, 0usize);
    let (mut in_q, mut qc) = (false, 0u8);
    for &b in bytes {
        if !in_q && (b == b'"' || b == b'\'') {
            in_q = true;
            qc = b;
            continue;
        }
        if in_q {
            if b == qc {
                in_q = false;
            }
            continue;
        }
        match b {
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'&' if bracket == 0 && paren == 0 => return true,
            _ => {}
        }
    }
    false
}

fn split_descendant_selector(selector: &str) -> Option<(&str, &str)> {
    let bytes = selector.as_bytes();
    let (mut bracket, mut paren) = (0usize, 0usize);
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b' ' | b'>' | b'+' | b'~' if bracket == 0 && paren == 0 => {
                let parent = selector[..i].trim();
                let child = selector[i..]
                    .trim()
                    .trim_start_matches([' ', '>', '+', '~'])
                    .trim();
                if !parent.is_empty() && !child.is_empty() {
                    return Some((parent, child));
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests;
