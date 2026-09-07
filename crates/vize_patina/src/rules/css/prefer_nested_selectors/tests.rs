//! The rule's own suite.

use super::PreferNestedSelectors;
use crate::rules::css::CssLinter;

fn create_linter() -> CssLinter {
    let mut linter = CssLinter::new();
    linter.add_rule(Box::new(PreferNestedSelectors));
    linter
}

#[test]
fn test_simple_selector() {
    let linter = create_linter();
    let result = linter.lint(".button { color: red; }", 0);
    assert_eq!(result.warning_count, 0);
}

#[test]
fn test_descendant_selector() {
    let linter = create_linter();
    let result = linter.lint(".parent .child { color: red; }", 0);
    assert_eq!(result.warning_count, 1);
}

#[test]
fn test_child_selector() {
    let linter = create_linter();
    let result = linter.lint(".parent > .child { color: red; }", 0);
    assert_eq!(result.warning_count, 1);
}

#[test]
fn test_element_descendant() {
    let linter = create_linter();
    let result = linter.lint("div span { color: red; }", 0);
    assert_eq!(result.warning_count, 1);
}

#[test]
fn test_attribute_selector() {
    let linter = create_linter();
    let result = linter.lint("[data-foo=\"bar baz\"] { color: red; }", 0);
    assert_eq!(result.warning_count, 0);
}

#[test]
fn test_nested_selector_list_does_not_warn() {
    // CSS nesting syntax: the `&` parent selector means the rule is already nested.
    // See https://github.com/ubugeeei-prod/vize/issues/2246.
    let linter = create_linter();
    let result = linter.lint(".rendered-content { & h1, & h2 { font-weight: 600; } }", 0);
    assert_eq!(
        result.warning_count, 0,
        "& h1, & h2 should not warn; diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_nested_selector_single_does_not_warn() {
    let linter = create_linter();
    let result = linter.lint(".parent { & .child { color: red; } }", 0);
    assert_eq!(
        result.warning_count, 0,
        "& .child should not warn; diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_keyframes_does_not_warn() {
    let linter = create_linter();
    let source = "@keyframes loading { 0% { opacity: 0; } 100% { opacity: 1; } }";
    let result = linter.lint(source, 0);
    assert_eq!(
        result.warning_count, 0,
        "@keyframes body should not warn; diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_import_does_not_warn() {
    let linter = create_linter();
    let source = "@import \"x.css\";\n.foo { color: red; }";
    let result = linter.lint(source, 0);
    assert_eq!(
        result.warning_count, 0,
        "@import should not warn; diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn test_font_face_does_not_warn() {
    let linter = create_linter();
    let result = linter.lint("@font-face { font-family: \"X\"; src: url(x.woff2); }", 0);
    assert_eq!(result.warning_count, 0);
}

#[test]
fn test_media_query_still_warns_on_descendants() {
    // Conditional group rules should still descend into their bodies.
    let linter = create_linter();
    let result = linter.lint(
        "@media (min-width: 600px) { .parent .child { color: red; } }",
        0,
    );
    assert_eq!(result.warning_count, 1);
}

/// The three shapes from the inverted-rule report: the rule fired on
/// CSS that was *already* nested and stayed silent on the flat
/// selectors it exists for. See
/// <https://github.com/ubugeeei-prod/vize/issues/5888>.
#[test]
fn already_nested_css_does_not_warn_and_flat_css_does() {
    let nested_with_at_rule = "\n.a {\n  color: red;\n\n  @media (hover: hover) {\n    &:hover {\n      color: blue;\n    }\n  }\n}\n";
    let flat = "\n.a {\n  color: red;\n}\n\n.a .b {\n  color: blue;\n}\n";
    let nested = "\n.a {\n  color: red;\n\n  .b {\n    color: blue;\n  }\n}\n";

    let linter = create_linter();
    assert_eq!(
        linter.lint(nested_with_at_rule, 0).warning_count,
        0,
        "a nested rule inside a nested at-rule is already nested",
    );
    assert_eq!(
        linter.lint(nested, 0).warning_count,
        0,
        "a nested rule is already nested",
    );

    let flat_result = linter.lint(flat, 0);
    assert_eq!(
        flat_result.warning_count, 1,
        "the flat selector is the one to report"
    );
    let reported = &flat_result.diagnostics[0];
    assert_eq!(
        &flat[reported.start as usize..reported.end as usize],
        ".a .b",
        "the span covers the selector alone",
    );
}

/// A declaration ends the prelude. Without that, the text before a
/// nested selector is read as part of it.
#[test]
fn a_declaration_is_not_part_of_the_next_selector() {
    let linter = create_linter();
    assert_eq!(
        linter
            .lint(".a { color: red; .b { color: blue; } }", 0)
            .warning_count,
        0,
    );
    assert_eq!(
        linter
            .lint(".a { background: url(\"a b.png\"); }", 0)
            .warning_count,
        0,
        "a space inside a quoted value is not a combinator",
    );
    assert_eq!(
        linter.lint(".a { /* a b */ color: red; }", 0).warning_count,
        0,
        "a space inside a comment is not a combinator",
    );
}

/// A group at-rule does not itself count as nesting, so a descendant
/// selector inside one still reports — but a style rule around it
/// does, at any depth.
#[test]
fn group_at_rules_do_not_count_as_nesting() {
    let linter = create_linter();
    assert_eq!(
        linter
            .lint("@supports (display: grid) { .a .b { color: red; } }", 0)
            .warning_count,
        1,
    );
    assert_eq!(
        linter
            .lint("@layer base { .a .b { color: red; } }", 0)
            .warning_count,
        1,
    );
    assert_eq!(
        linter
            .lint(".a { @media print { .b .c { color: red; } } }", 0)
            .warning_count,
        0,
        "already inside a style rule, however many at-rules intervene",
    );
}

#[test]
fn test_descendant_after_keyframes_still_warns() {
    let linter = create_linter();
    let source = "@keyframes loading { 0% { opacity: 0; } } .parent .child { color: red; }";
    let result = linter.lint(source, 0);
    assert_eq!(result.warning_count, 1);
}
