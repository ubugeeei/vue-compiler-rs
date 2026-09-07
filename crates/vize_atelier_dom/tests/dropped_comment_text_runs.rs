//! Text runs separated only by a **dropped** comment.
//!
//! Vue's parser never builds a comment node when `comments: false`, and
//! its `onText` appends to the previous child whenever that child is a
//! text node — without checking that the two are contiguous in source.
//! So with comments off, `a<!--c-->b` arrives at whitespace condensing
//! as the single text node `ab`, and `a\n<!--c-->\nb` as `a\n\nb`, which
//! condenses to `a b`. The comment is not a run boundary; it is not
//! there at all.
//!
//! With comments **on** the node exists and does break the run, which is
//! the shape `davinci_dom_corpus`'s comment fixtures already cover.
//!
//! Every expectation below is the output of `@vue/compiler-dom` itself
//! (checked against 3.5.41 and 3.6.0-beta.10, `mode: "function"`,
//! `comments: false`) and matches this crate's legacy parse/transform
//! lane. Only the two whitespace rows are a rendering difference rather
//! than a spelling one — `"a " + " b"` puts two spaces between the words
//! where Vue puts one.

#![allow(clippy::disallowed_macros, clippy::disallowed_types)]

use vize_atelier_dom::compile_template;
use vize_s0::Allocator;

/// `(name, template, the children argument Vue emits)`.
const CASES: &[(&str, &str, &str)] = &[
    ("static-tight", "<div>a<!--c-->b</div>", r#""ab""#),
    ("static-newlines", "<div>a\n<!--c-->\nb</div>", r#""a b""#),
    ("static-spaces", "<div>a <!--c--> b</div>", r#""a b""#),
    ("two-comments", "<div>a<!--c--><!--d-->b</div>", r#""ab""#),
    ("three-runs", "<div>a<!--c-->b<!--d-->c</div>", r#""abc""#),
    // Already correct before the fix — kept so a regression in the other
    // direction is just as loud.
    (
        "static-interp",
        "<div>a<!--c-->{{ b }}</div>",
        r#""a" + _toDisplayString(b), 1 /* TEXT */"#,
    ),
    (
        "interp-static",
        "<div>{{ a }}<!--c-->b</div>",
        r#"_toDisplayString(a) + "b", 1 /* TEXT */"#,
    ),
    (
        "interp-interp",
        "<div>{{ a }}<!--c-->{{ b }}</div>",
        r#"_toDisplayString(a) + _toDisplayString(b), 1 /* TEXT */"#,
    ),
    ("trailing-comment", "<div>a<!--c--></div>", r#""a""#),
    ("leading-comment", "<div><!--c-->a</div>", r#""a""#),
    // The condense rule reads the *comment-free* neighbours too, so a
    // whitespace run whose only non-text neighbours were comments is
    // condensed, never removed.
    (
        "newline-between-comments",
        "<p>a<!--c-->\n<!--d-->b</p>",
        r#""a b""#,
    ),
    (
        "space-between-comments",
        "<p>a<!--c--> <!--d-->b</p>",
        r#""a b""#,
    ),
    (
        "run-then-interpolation",
        "<p>a<!--c-->b {{ x }}</p>",
        r#""ab " + _toDisplayString(x), 1 /* TEXT */"#,
    ),
    // A text node whose own trailing (or leading) whitespace sits against
    // a dropped comment still condenses internally. The comment being
    // absorbed into the text group is what made these reachable: the
    // group holds two children and one text, so the per-node collapse has
    // to count *texts*, and its text is not always the group's first
    // child.
    (
        "text-then-trailing-comment",
        "<p>a\n<!--c--></p>",
        r#""a ""#,
    ),
    (
        "text-then-indented-trailing-comment",
        "<p>a\n  <!--c--></p>",
        r#""a ""#,
    ),
    ("leading-comment-then-text", "<p><!--c-->\na</p>", r#"" a""#),
    (
        "leading-comment-then-indented-text",
        "<p><!--c-->\n  a</p>",
        r#"" a""#,
    ),
];

/// The whole render body for `<div>a<!--c-->b</div>` with `comments: true`:
/// three children, the comment standing between the two text nodes.
const PRESERVED_COMMENT_RENDER: &str = "\
function render(_ctx, _cache, $props, $setup, $data, $options) {
  return (_openBlock(), _createElementBlock(\"div\", null, [
    _createTextVNode(\"a\"),
    _createCommentVNode(\"c\"),
    _createTextVNode(\"b\")
  ]))
}";

fn children_argument(code: &str) -> &str {
    let line = code
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("return "))
        .unwrap_or_else(|| panic!("no render return in:\n{code}"));
    let open = ["div", "p"]
        .iter()
        .find_map(|tag| {
            let needle = format!("_createElementBlock(\"{tag}\", null, ");
            line.find(&needle).map(|at| at + needle.len())
        })
        .unwrap_or_else(|| panic!("no element block in: {line}"));
    let rest = &line[open..];
    let close = rest
        .rfind("))")
        .unwrap_or_else(|| panic!("no block close in: {line}"));
    &rest[..close]
}

#[test]
fn a_dropped_comment_does_not_break_a_text_run() {
    for (name, source, expected) in CASES {
        let allocator = Allocator::new();
        let (_, errors, result) = compile_template(&allocator, source);
        assert!(errors.is_empty(), "{name} compiles cleanly");
        assert_eq!(
            children_argument(&result.code),
            *expected,
            "{name}: {source:?}",
        );
    }
}

/// The other half of the rule: a **preserved** comment is a real child,
/// so it does break the run and the neighbours stay separate.
#[test]
fn a_preserved_comment_still_breaks_the_run() {
    let options = vize_atelier_dom::DomCompilerOptions {
        comments: true,
        ..vize_atelier_dom::DomCompilerOptions::default()
    };
    let allocator = Allocator::new();
    let (_, errors, result) = vize_atelier_dom::compile_template_with_options(
        &allocator,
        "<div>a<!--c-->b</div>",
        options,
    );
    assert!(errors.is_empty());
    assert_eq!(result.code, PRESERVED_COMMENT_RENDER);
}
