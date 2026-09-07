use std::borrow::Cow;

use crate::types::{BindingMetadata, BlockLocation, SfcTemplateBlock, TemplateCompileOptions};

use super::extraction::{extract_template_parts, slice_template_parts};
use super::{TemplateBlockCompileContext, compile_template_block};

/// The emission-recorded section offsets must reproduce exactly what the
/// line scanner extracts, for every shape of generated render module the
/// SFC DOM path can produce.
#[test]
fn test_slice_template_parts_matches_line_scanner() {
    let templates = [
        // Plain element + interpolation
        "<div>{{ msg }}</div>",
        // Static-only content (hoistable, may need no assets)
        "<div><img style=\"position: absolute; top: 0\" alt=\"x\"></div>",
        // Unresolved component + directive => asset preamble lines
        "<MyWidget v-focus>{{ count + 1 }}</MyWidget>",
        // Root-level v-if/v-else => multi-line ternary return
        "<div v-if=\"shown\">a</div>\n<span v-else>b</span>",
        // Multi-root fragment
        "<header>h</header>\n<footer>f</footer>",
        // v-for with nested interpolation
        "<ul><li v-for=\"item in items\" :key=\"item.id\">{{ item.name }}</li></ul>",
        // Slot outlet root
        "<slot name=\"body\" :row=\"row\" />",
        // Event handlers (cached) + v-model
        "<input v-model=\"text\" @keyup.enter=\"submit($event)\">",
        // Plain text root
        "hello",
    ];

    for inline in [true, false] {
        for source in templates {
            let template = SfcTemplateBlock {
                content: Cow::Borrowed(source),
                loc: BlockLocation {
                    start: 0,
                    end: 0,
                    tag_start: 0,
                    tag_end: 0,
                    start_line: 1,
                    start_column: 1,
                    end_line: 1,
                    end_column: 1,
                },
                lang: None,
                src: None,
                attrs: Default::default(),
            };
            let bindings = BindingMetadata::default();
            let template_allocator = vize_s0::Allocator::new();
            let mut template_options = TemplateCompileOptions::default();
            if !inline {
                let mut compiler_options = vize_atelier_dom::DomCompilerOptions::default();
                compiler_options.hoist_static = false;
                template_options.compiler_options = Some(compiler_options);
            }
            let result = compile_template_block(
                &template_allocator,
                &template,
                &template_options,
                &vize_atelier_core::options::CustomElementMatcher::default(),
                TemplateBlockCompileContext {
                    scope_id: "abc123",
                    apply_scope_id: false,
                    has_scoped: true,
                    is_ts: false,
                    inline,
                    component_name: Some("TestComp"),
                    bindings: Some(&bindings),
                    croquis: None,
                },
                vize_atelier_core::TemplateSyntaxMode::Standard,
                &vize_atelier_core::CodegenOptions::default(),
            )
            .expect("template should compile");

            let sections = result
                .sections
                .expect("DOM lane must record section offsets");
            let sliced = slice_template_parts(&result.code, &sections);
            let scanned = extract_template_parts(&result.code);

            assert_eq!(
                sliced, scanned,
                "sliced sections must match the line scanner for inline={inline} template:\n{source}\n\ngenerated code:\n{}",
                result.code
            );
        }
    }
}
