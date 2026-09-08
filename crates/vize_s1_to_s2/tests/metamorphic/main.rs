//! Davinci P2-15 — the metamorphic suite v1 (TS-21).
//!
//! Equivalence-preserving mutations of Vue template source (attribute
//! reorder, pass-through `<template>` wrap, text-node split/merge,
//! whitespace-insignificant edits) must leave the S2 folio identical
//! modulo the declared normalization, compared as full normalized
//! artifacts. The per-mutator equivalence justifications live in
//! `mutators.rs`, the declared normalization in `normalize.rs`, the
//! exclusion predicates in `sites.rs`, and the scope-proof counting in
//! `runner.rs`.
//!
//! Two sources, both with the scope proof (mutations applied and skips
//! counted; a zero-mutation run fails):
//!
//! - the committed construct-matrix plane
//!   (`tests/fixtures/davinci-matrix/`, TS-12 keeps it fresh), pinned
//!   census, always on;
//! - a corpus shard via `VIZE_DAVINCI_METAMORPHIC_CORPUS=<dir>[,<dir>…]`
//!   (read-only, copies-in-memory only — the P0-13 convention), the
//!   P2-8 widening shape. CI feeds it the PR-hydrated submodules from
//!   `tests/tooling/davinci-metamorphic-corpus.test.ts`; the full-corpus
//!   recipe is the same variable pointed at `tests/_fixtures/_git`.

mod mutators;
mod normalize;
mod predicates;
mod runner;
mod sites;

use std::path::{Path, PathBuf};

use runner::{Report, collect_vue_files, run_files, run_source};

fn matrix_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/davinci-matrix")
}

#[test]
fn the_matrix_plane_is_metamorphically_stable() {
    let mut files = Vec::new();
    collect_vue_files(&matrix_dir(), &mut files);
    assert_eq!(
        files.len(),
        90,
        "the committed element-kind x directive plane moved; re-pin the census deliberately"
    );
    let report = run_files(&files);
    assert_eq!(report.failures, Vec::<vize_s0::String>::new());
    report
        .verdict("matrix plane")
        .unwrap_or_else(|message| panic!("{message}"));
    eprintln!("{}", report.scope_line("matrix plane"));
    // The pinned census: (sites, applied, skipped, beyond-cap) per
    // mutator over the 90 stubs. A move here is a mutator, predicate,
    // taxonomy or parser change — re-pin deliberately, in the record.
    let census: Vec<(u64, u64, u64, u64)> = [
        report.reorder,
        report.wrap,
        report.split,
        report.merge,
        report.whitespace,
    ]
    .iter()
    .map(|tally| (tally.sites, tally.applied, tally.skipped, tally.beyond_cap))
    .collect();
    assert_eq!(
        census,
        vec![
            (0, 0, 0, 0),      // reorder: stubs carry one attribute each
            (30, 27, 3, 0),    // wrap: the three template--v-else* targets skip
            (192, 102, 90, 0), // split: one unsplittable "\n" per stub skips
            (0, 0, 0, 0),      // merge: the tokenizer emits maximal runs
            (192, 192, 0, 0),  // whitespace: every indentation run mutates
        ],
        "matrix census moved: re-pin deliberately (see the P2-15 record)"
    );
}

#[test]
fn the_corpus_shard_is_metamorphically_stable() {
    let Some(roots) = std::env::var_os("VIZE_DAVINCI_METAMORPHIC_CORPUS") else {
        eprintln!("VIZE_DAVINCI_METAMORPHIC_CORPUS unset: matrix plane only");
        return;
    };
    let mut files = Vec::new();
    for root in roots.to_string_lossy().split(',') {
        let root = PathBuf::from(root);
        // Cargo runs test binaries from the package directory; let
        // workspace-root-relative paths work too (the P2-8 lane's rule).
        let root = if root.is_relative() && !root.is_dir() {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(&root)
        } else {
            root
        };
        assert!(
            root.is_dir(),
            "VIZE_DAVINCI_METAMORPHIC_CORPUS must name directories: {}",
            root.display()
        );
        collect_vue_files(&root, &mut files);
    }
    assert!(!files.is_empty(), "corpus shard found no .vue files");
    let report = run_files(&files);
    assert_eq!(report.failures, Vec::<vize_s0::String>::new());
    report
        .verdict("corpus shard")
        .unwrap_or_else(|message| panic!("{message}"));
    eprintln!("{}", report.scope_line("corpus shard"));
}

#[test]
fn a_zero_mutation_run_fails() {
    // One unsplittable text node: one site, one counted skip, zero
    // mutations — the verdict must be the failure, not a quiet pass.
    let mut report = Report::default();
    run_source("<template>x</template>", "degenerate", &mut report);
    assert_eq!(report.mutations(), 0);
    assert_eq!(report.split.sites, 1);
    assert_eq!(report.split.skipped, 1);
    assert_eq!(
        report.verdict("degenerate"),
        Err(vize_s0::String::from(
            "metamorphic degenerate: zero mutations were applied — the run proves nothing \
             (1 files, 1 sites, 1 skips); a degenerated suite must fail, not pass"
        ))
    );
    // An empty file list degenerates the same way.
    let empty = run_files(&[]);
    assert_eq!(
        empty.verdict("empty"),
        Err(vize_s0::String::from(
            "metamorphic empty: zero mutations were applied — the run proves nothing \
             (0 files, 0 sites, 0 skips); a degenerated suite must fail, not pass"
        ))
    );
}

#[test]
fn the_normalization_does_not_collapse_real_differences() {
    // Since the P2-9 ratchet the only declared rule is attribute
    // sorting; the text-merge and condense checks below hold with **no
    // normalization at all** — they now pin that the lowering's own
    // canonical operations (`lower::text`) keep real differences apart.
    use normalize::{Normalization, normalized_display};
    let folio = |source: &str| {
        let allocator = vize_s0::Allocator::new();
        let (tree, errors) = vize_s1::parse(&allocator, source);
        let lowered = vize_s1_to_s2::lower(&allocator, &tree, &errors);
        vize_s2::folio::DisegnoFolio::of(&lowered.root.ops)
    };
    // Attribute sorting must not erase a value difference…
    assert_ne!(
        normalized_display(
            &folio(r#"<template><div a="1" b="2"/></template>"#),
            Normalization::REORDER
        ),
        normalized_display(
            &folio(r#"<template><div a="2" b="1"/></template>"#),
            Normalization::REORDER
        ),
    );
    // …the lowering's text merge must not erase an element boundary…
    assert_ne!(
        normalized_display(&folio("<template>ab</template>"), Normalization::NONE),
        normalized_display(
            &folio("<template>a<i></i>b</template>"),
            Normalization::NONE
        ),
    );
    // …and the lowering's condense must not erase a text difference.
    assert_ne!(
        normalized_display(
            &folio("<template><i>a b</i></template>"),
            Normalization::NONE
        ),
        normalized_display(
            &folio("<template><i>a c</i></template>"),
            Normalization::NONE
        ),
    );
}

#[test]
fn each_mutator_holds_on_a_handwritten_pin() {
    // One hand-written source per mutator, so a matrix regeneration can
    // never silently retire a mutator's only coverage.
    let cases: Vec<(&str, &str)> = vec![
        // reorder: two swappable statics; the (id, class) pair skips.
        (
            "reorder",
            "<template><div title=\"t\" id=\"i\" class=\"c\"></div></template>",
        ),
        // wrap: a root-position branch chain, both elements wrappable.
        (
            "wrap",
            "<template><div v-if=\"c\">a</div><span v-else>b</span></template>",
        ),
        // split + whitespace: mixed text with interior runs.
        (
            "text",
            "<template><p>hello  world</p>\n  <p>again</p></template>",
        ),
    ];
    let mut report = Report::default();
    for (label, source) in &cases {
        run_source(source, label, &mut report);
    }
    assert_eq!(report.failures, Vec::<vize_s0::String>::new());
    let census: Vec<(u64, u64, u64, u64)> = [
        report.reorder,
        report.wrap,
        report.split,
        report.merge,
        report.whitespace,
    ]
    .iter()
    .map(|tally| (tally.sites, tally.applied, tally.skipped, tally.beyond_cap))
    .collect();
    assert_eq!(
        census,
        vec![
            (2, 1, 1, 0), // (title,id) applied; (id,class) skips per merge-order family
            (2, 2, 0, 0), // both branch elements wrap
            (5, 3, 2, 0), // "a"/"b" are unsplittable, the three long texts split
            (0, 0, 0, 0),
            (2, 2, 0, 0), // "hello  world" interior run + the inter-element run
        ],
        "hand-written pin census moved: re-pin deliberately"
    );
}

#[test]
fn the_merge_mutator_holds_through_its_own_site_path() {
    // The tokenizer emits maximal text runs, so parser output never
    // carries adjacent text nodes and merge's natural census is zero
    // (measured over the full corpus; recorded in the P2-15 record).
    // Its full path — enumeration finds the adjacency, the predicates
    // pass, `apply` merges, the oracle compares — is exercised here on
    // a split tree, exactly the shape a recovered input would present.
    let source = "<template>hello world</template>";
    let allocator = vize_s0::Allocator::new();
    let (mut tree, errors) = vize_s1::parse(&allocator, source);
    let original = {
        let lowered = vize_s1_to_s2::lower(&allocator, &tree, &errors);
        vize_s2::folio::DisegnoFolio::of(&lowered.root.ops)
    };
    mutators::split_text(&mut tree, &[0, 0], 5);
    let merge_sites: Vec<sites::Site> = sites::enumerate(&tree)
        .into_iter()
        .filter(|site| site.kind == sites::Kind::Merge)
        .collect();
    assert_eq!(merge_sites.len(), 1);
    assert_eq!(merge_sites[0].skip, None);
    let mutant = mutators::apply(&allocator, &mut tree, &merge_sites[0]);
    assert!(matches!(mutant, mutators::Mutant::InPlace));
    let lowered = vize_s1_to_s2::lower(&allocator, &tree, &errors);
    let merged = vize_s2::folio::DisegnoFolio::of(&lowered.root.ops);
    // The re-merged slice is the original maximal run: byte-identical
    // `Full`-mode folios, no normalization needed at all.
    use vize_davinci::folio::{Folio, FolioMode};
    assert_eq!(
        merged.print_to_string(FolioMode::Full),
        original.print_to_string(FolioMode::Full)
    );
}

#[test]
fn the_exclusion_predicates_refuse_the_documented_unsafe_sites() {
    let skips = |source: &str| {
        let allocator = vize_s0::Allocator::new();
        let (tree, _errors) = vize_s1::parse(&allocator, source);
        sites::enumerate(&tree)
            .into_iter()
            .filter_map(|site| site.skip.map(|reason| (site.kind, reason)))
            .collect::<Vec<_>>()
    };
    use sites::Kind;
    // A template target refuses the wrap; a keyed branch carrier and a
    // slot child refuse it too.
    assert_eq!(
        skips("<template><template v-if=\"c\">x</template></template>"),
        vec![
            (Kind::Wrap, "template-tag"),
            (Kind::Split, "no-split-point")
        ]
    );
    assert_eq!(
        skips("<template><a v-if=\"prev\" key=\"prev\">p</a></template>"),
        vec![
            (Kind::Reorder, "directive-attr"),
            (Kind::Wrap, "branch-key-carrier"),
            (Kind::Split, "no-split-point"),
        ]
    );
    assert_eq!(
        skips("<template><Comp><div v-if=\"c\"></div></Comp></template>"),
        vec![(Kind::Wrap, "slot-content")]
    );
    // class/style freeze the pair; duplicate names freeze the pair.
    assert_eq!(
        skips("<template><div class=\"a\" style=\"b\"></div></template>"),
        vec![(Kind::Reorder, "merge-order-family")]
    );
    assert_eq!(
        skips("<template><div data-x=\"1\" DATA-X=\"2\"></div></template>"),
        vec![(Kind::Reorder, "duplicate-name")]
    );
    // v-pre freezes its subtree for every mutator; pre freezes whitespace.
    assert_eq!(
        skips("<template><div v-pre><span v-if=\"c\">a b</span></div></template>"),
        vec![
            (Kind::Wrap, "v-pre-subtree"),
            (Kind::Split, "v-pre-subtree"),
            (Kind::Whitespace, "v-pre-subtree"),
        ]
    );
    assert_eq!(
        skips("<template><pre>a b</pre></template>"),
        vec![(Kind::Whitespace, "pre-content")]
    );
    // Rawtext content is out of every mutator's scope.
    assert_eq!(
        skips("<template><textarea>a b</textarea></template>"),
        vec![
            (Kind::Split, "rawtext-content"),
            (Kind::Whitespace, "rawtext-content"),
        ]
    );
    // A root <template> with attributes is not template scope at all.
    assert_eq!(skips("<template lang=\"pug\">div a b</template>"), vec![]);
}
