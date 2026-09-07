//! Every adjacency of the text family, against the legacy lane.
//!
//! The corpus lanes compare 12,062 real templates and still missed three
//! comment-adjacent text bugs, because no real component writes
//! `a<!--c-->b`: comments in templates sit on their own lines between
//! elements. The shapes that broke are trivially *enumerable* even though
//! they are rare, so this enumerates them — every ordered pair of text,
//! interpolation, comment and element atoms, over every whitespace gap,
//! plus the triples through a comment and the leading/trailing cases.
//!
//! The oracle is `compile_template_legacy_with_options`, the shipped
//! parse/transform/codegen lane. Spot-checked against `@vue/compiler-dom`
//! 3.5.41 and 3.6.0-beta.10 too: the two agree on every shape here except
//! Vue 3.6's per-child static caching (`_cache[n] || (_cache[n] = …)`),
//! which neither vize lane emits.

#![allow(clippy::disallowed_macros, clippy::disallowed_types)]

use vize_atelier_dom::{
    DomCompilerOptions, compile_template, compile_template_legacy_with_options,
};
use vize_s0::Allocator;

/// The children that can sit inside the wrapper, one per kind the text
/// rules distinguish: two texts (so a merge is observable), two
/// interpolations, two comments, and two elements (empty and non-empty).
const ATOMS: [(&str, &str); 8] = [
    ("t", "a"),
    ("u", "b"),
    ("i", "{{ x }}"),
    ("j", "{{ y }}"),
    ("c", "<!--c-->"),
    ("d", "<!--d-->"),
    ("e", "<span></span>"),
    ("f", "<i>z</i>"),
];

/// The gaps, covering each branch of the condense rule: none, a space
/// (no newline — condensed, never removed), a bare newline, an indented
/// newline, and a run with whitespace on both sides of one.
const GAPS: [(&str, &str); 5] = [
    ("0", ""),
    ("s", " "),
    ("n", "\n"),
    ("m", "\n  "),
    ("w", "  \n  "),
];

fn render_body(code: &str) -> String {
    code.lines()
        .map(str::trim)
        .skip_while(|line| !line.starts_with("return "))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compile `source` through both lanes and require them byte-identical.
fn assert_lanes_agree(name: &str, source: &str, compared: &mut usize) {
    let s2_allocator = Allocator::new();
    let (_, errors, s2) = compile_template(&s2_allocator, source);
    assert!(
        errors.is_empty(),
        "{name}: {source:?} should compile cleanly"
    );

    let legacy_allocator = Allocator::new();
    let (_, legacy_errors, legacy) = compile_template_legacy_with_options(
        &legacy_allocator,
        source,
        DomCompilerOptions::default(),
    );
    // A lane can report an error and still emit recovery code that
    // happens to match, which would let a real divergence through.
    assert!(
        legacy_errors.is_empty(),
        "{name}: the legacy lane should compile {source:?} cleanly: {legacy_errors:?}",
    );

    assert_eq!(
        render_body(&s2.code),
        render_body(&legacy.code),
        "{name}: {source:?}",
    );
    assert_eq!(s2.preamble, legacy.preamble, "{name} preamble: {source:?}");
    *compared += 1;
}

#[test]
fn every_text_family_adjacency_agrees_with_the_legacy_lane() {
    let mut compared = 0usize;

    // Ordered pairs: `<div>A<gap>B</div>` for every A != B.
    for (a_key, a) in ATOMS {
        for (b_key, b) in ATOMS {
            if a_key == b_key {
                continue;
            }
            for (gap_key, gap) in GAPS {
                assert_lanes_agree(
                    &format!("pair {a_key}{gap_key}{b_key}"),
                    &format!("<div>{a}{gap}{b}</div>"),
                    &mut compared,
                );
            }
        }
    }

    // Triples through a comment — the shape a dropped comment makes into
    // one text run, and the one a preserved comment splits. The two gaps
    // vary **independently**: an equal pair never produces
    // `<i/><!--c--> <b/>`, where the comment opens the whitespace group,
    // and that asymmetry is exactly the shape the first cut got wrong.
    for (a_key, a) in ATOMS {
        for (b_key, b) in ATOMS {
            for (left_key, left) in GAPS {
                for (right_key, right) in GAPS {
                    assert_lanes_agree(
                        &format!("comment3 {a_key}{left_key}|{right_key}{b_key}"),
                        &format!("<div>{a}{left}<!--sep-->{right}{b}</div>"),
                        &mut compared,
                    );
                }
            }
        }
    }

    // Leading and trailing whitespace around a single atom.
    for (a_key, a) in ATOMS {
        for (gap_key, gap) in GAPS {
            assert_lanes_agree(
                &format!("edges {a_key}{gap_key}"),
                &format!("<div>{gap}{a}{gap}</div>"),
                &mut compared,
            );
        }
    }

    // The scope proof: a matrix that quietly stopped generating shapes
    // would pass every assertion above (assurance §4's zero-mutation
    // failure mode).
    assert_eq!(
        compared,
        ATOMS.len() * (ATOMS.len() - 1) * GAPS.len()
            + ATOMS.len() * ATOMS.len() * GAPS.len() * GAPS.len()
            + ATOMS.len() * GAPS.len(),
        "the matrix must cover every pair, triple and edge case",
    );
    assert_eq!(compared, 1920);
}

/// The same matrix with comments **preserved**, where a comment is a real
/// child and does break a text run.
#[test]
fn the_adjacencies_agree_with_comments_preserved_too() {
    let options = || DomCompilerOptions {
        comments: true,
        ..DomCompilerOptions::default()
    };
    let mut compared = 0usize;
    for (a_key, a) in ATOMS {
        for (b_key, b) in ATOMS {
            for (gap_key, gap) in GAPS {
                let source = format!("<div>{a}{gap}<!--sep--> {b}</div>");
                let s2_allocator = Allocator::new();
                let (_, errors, s2) = vize_atelier_dom::compile_template_with_options(
                    &s2_allocator,
                    &source,
                    options(),
                );
                assert!(errors.is_empty(), "{a_key}{gap_key}{b_key}: {source:?}");
                let legacy_allocator = Allocator::new();
                let (_, legacy_errors, legacy) =
                    compile_template_legacy_with_options(&legacy_allocator, &source, options());
                assert!(legacy_errors.is_empty(), "legacy lane: {source:?}");
                assert_eq!(
                    render_body(&s2.code),
                    render_body(&legacy.code),
                    "preserved {a_key}{gap_key}{b_key}: {source:?}",
                );
                compared += 1;
            }
        }
    }
    assert_eq!(compared, ATOMS.len() * ATOMS.len() * GAPS.len());
}
