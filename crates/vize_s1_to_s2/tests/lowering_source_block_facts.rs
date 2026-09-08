//! Source-block lowering facts: side tables and provenance keep file-absolute
//! spans after SFC block slicing.

mod support;

use support::{Artifact, assert_authored_artifact};
use vize_davinci::folio::{Folio, FolioMode};
use vize_davinci::id::NodeId;
use vize_davinci::side_table::SideTable;
use vize_s0::{Allocator, SourceRoot, Span, String};
use vize_s2::folio::DisegnoFolio;
use vize_s2::provenance::ProvenanceRecord;
use vize_s2::scope::{ScopeBinding, ScopeFacts, ScopeOrigin, ScopeTag};
use vize_s2::verify::{Rigor, Violation, verify, verify_table};

fn authored(name: &str, start: u32, end: u32) -> ScopeBinding {
    ScopeBinding {
        name: String::from(name),
        origin: ScopeOrigin::Authored {
            span: Span::new(start, end),
        },
    }
}

fn span_of(source: &str, needle: &str) -> Span {
    let start = source.find(needle).expect("needle exists") as u32;
    Span::new(start, start + needle.len() as u32)
}

fn element_span(source: &str, open: &str, close: &str) -> Span {
    let start = source.find(open).expect("open tag exists") as u32;
    let end = source.find(close).expect("close tag exists") + close.len();
    Span::new(start, end as u32)
}

fn source_block_artifact(source: &str, block_source: &str, block_start: usize) -> Artifact {
    let allocator = Allocator::new();
    let (tree, errors) = vize_s1::parse(&allocator, block_source);
    let root = SourceRoot::new(source).expect("source is small");
    let block = root
        .block(block_source, block_start as u32)
        .expect("block is an exact root slice");
    let lowered = vize_s1_to_s2::lower_source_block(&allocator, &tree, &errors, block);
    let folio = DisegnoFolio::of(&lowered.root.ops);

    assert_authored_artifact(source, &lowered);
    assert_eq!(u64::from(lowered.op_count), folio.op_count());
    assert_eq!(verify(&folio, Rigor::Canonical), Vec::<Violation>::new());
    assert_eq!(
        verify_table(&folio, &lowered.scopes),
        Vec::<Violation>::new()
    );
    let provenance_ids: SideTable<()> = lowered
        .provenance
        .iter()
        .filter_map(|record| record.node.map(|node| (node, ())))
        .collect();
    assert_eq!(
        verify_table(&folio, &provenance_ids),
        Vec::<Violation>::new()
    );

    Artifact {
        folio: folio.print_to_string(FolioMode::Full),
        op_count: lowered.op_count,
        diagnostics: lowered.diagnostics.clone(),
        provenance: lowered.provenance.clone(),
        scopes: lowered
            .scopes
            .sorted_entries()
            .into_iter()
            .map(|(id, facts)| (id.index(), facts.clone()))
            .collect(),
    }
}

#[test]
fn source_block_side_tables_use_file_absolute_spans() {
    let source = concat!(
        "<script setup>const items=[]</script>",
        "<template>",
        "<ul><li v-for=\"(item, i) in items\" v-if=\"item.ok\">{{ item.name }}</li></ul>",
        "</template>",
    );
    let block_start = source.find("<ul>").expect("template content exists");
    let block_end = source.find("</template>").expect("template close exists");
    let block_source = &source[block_start..block_end];
    let art = source_block_artifact(source, block_source, block_start);

    let aliases = span_of(source, "(item, i)");
    let item_span = Span::new(aliases.start + 1, aliases.start + 5);
    let index_span = Span::new(aliases.start + 7, aliases.start + 8);
    assert!(item_span.start > block_start as u32);
    assert_eq!(
        art.scopes,
        vec![(
            2,
            ScopeFacts {
                tag: ScopeTag::from_index(0),
                bindings: vec![
                    authored("item", item_span.start, item_span.end),
                    authored("i", index_span.start, index_span.end),
                ],
            },
        )]
    );

    let ul_span = element_span(source, "<ul>", "</ul>");
    let li_open = r#"<li v-for="(item, i) in items" v-if="item.ok">"#;
    let li_span = element_span(source, li_open, "</li>");
    assert!(li_span.start > block_start as u32);
    assert_eq!(
        art.provenance,
        vec![
            ProvenanceRecord {
                rule: String::from("lower.element"),
                node: NodeId::from_index(0),
                before: String::from("<ul>"),
                after: String::from("ui.element ul"),
                span: ul_span,
            },
            ProvenanceRecord {
                rule: String::from("lower.if"),
                node: NodeId::from_index(1),
                before: String::from(r#"v-if="item.ok""#),
                after: String::from("ui.if branches=1"),
                span: li_span,
            },
            ProvenanceRecord {
                rule: String::from("lower.for"),
                node: NodeId::from_index(2),
                before: String::from("(item, i) in items"),
                after: String::from("ui.for source=js value=js"),
                span: span_of(source, "(item, i) in items"),
            },
            ProvenanceRecord {
                rule: String::from("lower.for-fact"),
                node: NodeId::from_index(2),
                before: String::from("scope #0 bindings=2"),
                after: String::from("fact value=item key=i index=-"),
                span: span_of(source, "(item, i) in items"),
            },
            ProvenanceRecord {
                rule: String::from("lower.element"),
                node: NodeId::from_index(3),
                before: String::from(li_open),
                after: String::from("ui.element li"),
                span: li_span,
            },
            ProvenanceRecord {
                rule: String::from("lower.interpolation"),
                node: NodeId::from_index(4),
                before: String::from(" item.name "),
                after: String::from("ui.interpolation js"),
                span: span_of(source, "{{ item.name }}"),
            },
        ]
    );
    assert_eq!(art.diagnostics, Vec::new());
    assert_eq!(art.op_count, 5);
}
