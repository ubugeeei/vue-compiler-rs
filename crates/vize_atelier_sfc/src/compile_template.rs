//! Template compilation for Vue SFCs.
//!
//! This module handles compilation of `<template>` blocks,
//! supporting both DOM mode and Vapor mode.

use vize_s0::{String, ToCompactString, profile};
mod extraction;
mod string_tracking;
mod vapor;

#[cfg(test)]
mod section_offsets_tests;
#[cfg(test)]
mod tests;

pub(crate) use extraction::{
    extract_template_parts, extract_template_parts_full, slice_template_parts,
};
pub(crate) use vapor::compile_template_block_vapor;

use vize_atelier_core::CodegenOptions;
use vize_atelier_core::TemplateSyntaxMode;
use vize_s0::Allocator;

use vize_atelier_core::CompilerErrorWithSource;

use crate::compile::output_module::OutputModule;
use crate::types::{BindingMetadata, SfcError, SfcTemplateBlock, TemplateCompileOptions};
use vize_atelier_core::options::CustomElementMatcher;

pub(crate) struct TemplateBlockCompileResult {
    pub(crate) code: String,
    pub(crate) warnings: std::vec::Vec<SfcError>,
    /// Section boundaries of `code`, recorded while the render module was
    /// emitted. `None` for the SSR/Vapor lanes (they re-scan via
    /// `extract_template_parts_full`) and for codegen error paths.
    pub(crate) sections: Option<TemplateCodeSections>,
}

/// Byte ranges into [`TemplateBlockCompileResult::code`] marking the sections
/// the inline SFC assembly consumes. Mirrors what
/// [`extraction::extract_template_parts`] reconstructs by line scanning, but
/// recorded at emission so the hot path can slice instead of re-scan.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TemplateCodeSections {
    /// `import { ... } from "vue"` line(s), trailing newline included.
    pub(crate) imports: (usize, usize),
    /// Hoisted module-level declarations, one per line.
    pub(crate) hoisted: (usize, usize),
    /// Component/directive resolution statements inside the render function
    /// (raw slice; lines carry codegen indentation).
    pub(crate) assets: (usize, usize),
    /// The root `return` expression of the render function.
    pub(crate) return_expr: (usize, usize),
}

pub(crate) struct TemplateBlockCompileContext<'a> {
    pub(crate) scope_id: &'a str,
    pub(crate) apply_scope_id: bool,
    /// Whether the component has any `<style scoped>` block. When true, hoisted
    /// module-level static vnodes must carry the `data-v-*` attribute so scoped
    /// CSS selectors continue to match them in client builds.
    pub(crate) has_scoped: bool,
    pub(crate) is_ts: bool,
    pub(crate) inline: bool,
    pub(crate) component_name: Option<&'a str>,
    pub(crate) bindings: Option<&'a BindingMetadata>,
    pub(crate) croquis: Option<vize_croquis::croquis::Croquis>,
}

/// Compile template block
pub(crate) fn compile_template_block(
    allocator: &Allocator,
    template: &SfcTemplateBlock,
    options: &TemplateCompileOptions,
    custom_elements: &CustomElementMatcher,
    ctx: TemplateBlockCompileContext<'_>,
    template_syntax: TemplateSyntaxMode,
    codegen_options: &CodegenOptions,
) -> Result<TemplateBlockCompileResult, SfcError> {
    let TemplateBlockCompileContext {
        scope_id,
        apply_scope_id,
        has_scoped,
        is_ts,
        inline,
        component_name,
        bindings,
        croquis,
    } = ctx;
    let compiler_options = options.compiler_options.as_ref();
    let scope_attr = if apply_scope_id {
        let mut attr = String::with_capacity(scope_id.len() + 7);
        attr.push_str("data-v-");
        attr.push_str(scope_id);
        Some(attr)
    } else {
        None
    };

    if options.ssr {
        let ssr_opts = vize_atelier_ssr::SsrCompilerOptions {
            scope_id: scope_attr,
            component_name: component_name.map(|name| name.to_compact_string()),
            comments: compiler_options.is_some_and(|opts| opts.comments),
            experimental_in_tag_comments: compiler_options
                .is_some_and(|opts| opts.experimental_in_tag_comments),
            experimental_patterned_template: compiler_options
                .is_some_and(|opts| opts.experimental_patterned_template),
            inline: false,
            is_ts,
            custom_renderer: options.custom_renderer,
            ssr_css_vars: options.ssr_css_vars.clone(),
            dialect: options.dialect,
            binding_metadata: bindings.cloned(),
            croquis: croquis.map(Box::new),
        };

        let (_, errors, result) = profile!(
            "atelier.sfc.template.ssr",
            vize_atelier_ssr::compile_ssr_with_custom_elements_and_template_syntax(
                allocator,
                &template.content,
                ssr_opts,
                template_syntax,
                custom_elements.clone(),
            )
        );

        // Recoverable parser diagnostics (e.g. duplicate attribute) must
        // not gate SFC compilation, or a single `<div id=a id=b>` produces
        // a 0-byte module marked as success. (#958)
        let fatal: Vec<_> = errors
            .iter()
            .filter(|e| !e.is_recoverable())
            .map(|e| CompilerErrorWithSource::new(e, &template.content))
            .collect();
        if !fatal.is_empty() {
            let mut message = String::from("Template compilation errors: ");
            use std::fmt::Write as _;
            let _ = write!(&mut message, "{:?}", fatal);
            return Err(SfcError {
                message,
                code: Some("TEMPLATE_ERROR".to_compact_string()),
                loc: Some(template.loc.clone()),
            });
        }

        let output = OutputModule::from_render_chunks(result.preamble, result.code).into_code();
        return Ok(TemplateBlockCompileResult {
            code: output,
            warnings: recoverable_template_warnings(&errors),
            sections: None,
        });
    }

    // Build DOM compiler options
    let mut dom_opts = options.compiler_options.clone().unwrap_or_default();
    dom_opts.mode = vize_atelier_core::options::CodegenMode::Module;
    dom_opts.prefix_identifiers = true;
    // Vue applies SFC scope IDs at runtime. Only module-level hoisted VNodes
    // need an explicit scope attr baked into their props.
    dom_opts.scope_id = None;
    // Hoisted module-level static vnodes are created at import time, when the
    // runtime's `currentScopeId` is null, so the runtime cannot stamp the
    // scoped-CSS attribute on them. Bake `data-v-*` directly into their props
    // here whenever the component owns a scoped style block.
    let hoisted_scope_attr = if has_scoped {
        let mut attr = String::with_capacity(scope_id.len() + 7);
        attr.push_str("data-v-");
        attr.push_str(scope_id);
        Some(attr)
    } else {
        None
    };
    dom_opts.ssr = options.ssr;
    dom_opts.is_ts = is_ts;
    dom_opts.custom_renderer = options.custom_renderer;
    dom_opts.dialect = options.dialect;
    dom_opts.component_name = component_name.map(|name| name.to_compact_string());

    // For script setup, use inline mode to match Vue's actual compiler behavior
    // Inline mode generates direct closure references (e.g., msg instead of $setup.msg)
    // which are captured in the setup() function scope
    if inline && bindings.is_some() {
        dom_opts.inline = true;
        dom_opts.cache_handlers = true;
    }

    // Pass binding metadata directly (no string conversion needed)
    dom_opts.binding_metadata = bindings.cloned();

    // Pass Croquis to DOM compiler for enhanced transforms
    if let Some(c) = croquis {
        dom_opts.croquis = Some(Box::new(c));
    }

    // Compile template. SFC assembly consumes only diagnostics and emitted
    // render code, so use the section-only entry when it can preserve the
    // requested semantics. Module-mode static hoisting still stays on the
    // compatibility path until S2's hoist analysis covers the full DOM lane.
    let route_through_sections = inline || !dom_opts.hoist_static;
    let (errors, result) = if route_through_sections {
        profile!(
            "atelier.sfc.template.dom",
            vize_atelier_dom::compile_sfc_template_with_custom_elements_and_template_syntax_and_hoisted_scope_id_with_sections_and_codegen_options(
                allocator,
                &template.content,
                dom_opts,
                template_syntax,
                hoisted_scope_attr,
                custom_elements.clone(),
                codegen_options.clone(),
            )
        )
    } else {
        let (_, errors, result) = profile!(
            "atelier.sfc.template.dom",
            vize_atelier_dom::compile_template_with_custom_elements_and_template_syntax_and_hoisted_scope_id_and_codegen_options(
                allocator,
                &template.content,
                dom_opts,
                template_syntax,
                hoisted_scope_attr,
                custom_elements.clone(),
                codegen_options.clone(),
            )
        );
        (
            errors,
            vize_atelier_core::codegen::CodegenResultWithSections {
                result,
                sections: None,
            },
        )
    };

    // See above — drop recoverable parser diagnostics from the gating
    // check so duplicate-attribute SFCs still produce valid render code. (#958)
    let fatal: Vec<_> = errors
        .iter()
        .filter(|e| !e.is_recoverable())
        .map(|e| CompilerErrorWithSource::new(e, &template.content))
        .collect();
    if !fatal.is_empty() {
        let mut message = String::from("Template compilation errors: ");
        use std::fmt::Write as _;
        let _ = write!(&mut message, "{:?}", fatal);
        return Err(SfcError {
            message,
            code: Some("TEMPLATE_ERROR".to_compact_string()),
            loc: Some(template.loc.clone()),
        });
    }

    // Translate the emission-recorded section offsets into the concatenated
    // output: `output = preamble + '\n' + code + '\n'`, where `preamble` is
    // the import statement followed (when hoists exist) by '\n' + hoists.
    let output_module =
        OutputModule::from_render_chunks(result.result.preamble, result.result.code);
    let preamble_len = output_module.imports.len();
    let fn_base = output_module.function_base_offset();
    let sections = result.sections.map(|s| TemplateCodeSections {
        imports: (0, s.imports_len),
        hoisted: if preamble_len > s.imports_len {
            (s.imports_len + 1, preamble_len)
        } else {
            (preamble_len, preamble_len)
        },
        assets: (fn_base + s.assets_start, fn_base + s.assets_end),
        return_expr: (fn_base + s.return_expr_start, fn_base + s.return_expr_end),
    });
    let output = output_module.into_code();

    Ok(TemplateBlockCompileResult {
        code: output,
        warnings: recoverable_template_warnings(&errors),
        sections,
    })
}

pub(crate) fn recoverable_template_warnings(
    errors: &[vize_atelier_core::CompilerError],
) -> std::vec::Vec<SfcError> {
    errors
        .iter()
        .filter(|error| error.is_recoverable())
        .cloned()
        .map(Into::into)
        .collect()
}
