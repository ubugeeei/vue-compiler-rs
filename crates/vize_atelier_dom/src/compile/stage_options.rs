//! Per-stage option construction for DOM template compilation.
//!
//! Keeps the parse/transform option wiring out of `compile.rs` so that entry
//! point stays focused on pipeline flow.

use vize_atelier_core::codegen::{CodegenResult, CodegenResultWithSections, CodegenSections};
use vize_atelier_core::options::{
    BindingMetadata, BindingType, CodegenMode, CodegenOptions, CustomElementMatcher, ParserOptions,
    TemplateSyntaxMode, TransformOptions,
};
use vize_atelier_core::walk_probe::WalkCounts;
use vize_s0::Allocator;
use vize_s0::profiler::global_profiler;
use vize_s1_to_s2::{
    BindingKind, BindingTable, DomEmitMode, DomEmitOptions, DomEmitSections, EmitError, LegacyCaps,
};

use super::pipeline::S2EmitSelection;
use crate::namespace::get_namespace;
use crate::options::DomCompilerOptions;

/// Parser options with DOM-specific settings.
pub(super) fn parser_options(options: &DomCompilerOptions) -> ParserOptions {
    ParserOptions {
        is_void_tag: vize_s0::is_void_tag,
        is_native_tag: Some(vize_s0::is_native_tag),
        custom_renderer: options.custom_renderer,
        is_pre_tag: |tag| tag == "pre",
        get_namespace,
        comments: options.comments,
        experimental_in_tag_comments: options.experimental_in_tag_comments,
        dialect: options.dialect,
        ..ParserOptions::default()
    }
}

/// Transform options for the DOM-specific transform steps.
///
/// `BindingMetadata` is passed directly (no string conversion needed).
pub(super) fn transform_options(options: &DomCompilerOptions) -> TransformOptions {
    TransformOptions {
        prefix_identifiers: options.prefix_identifiers,
        hoist_static: options.hoist_static,
        cache_handlers: options.cache_handlers,
        scope_id: options.scope_id.clone(),
        ssr: options.ssr,
        is_ts: options.is_ts,
        inline: options.inline,
        custom_renderer: options.custom_renderer,
        experimental_patterned_template: options.experimental_patterned_template,
        binding_metadata: options.binding_metadata.clone(),
        dialect: options.dialect,
        ..Default::default()
    }
}

pub(super) fn codegen_options(
    options: &DomCompilerOptions,
    defaults: CodegenOptions,
) -> CodegenOptions {
    CodegenOptions {
        mode: options.mode,
        source_map: options.source_map,
        component_name: options.component_name.clone(),
        scope_id: options.scope_id.clone(),
        ssr: options.ssr,
        is_ts: options.is_ts,
        inline: options.inline,
        cache_handlers: options.cache_handlers,
        binding_metadata: options.binding_metadata.clone(),
        // Compound dynamic `v-bind` / `v-on` keys (`:[prefix+suffix]`) only
        // walk identifiers when this flag is set. Transform already receives
        // it; omitting it here left SFC module-mode render functions with
        // bare `prefix+suffix` and a runtime ReferenceError.
        prefix_identifiers: options.prefix_identifiers,
        ..defaults
    }
}

pub(super) fn s2_emit_supported(
    options: &DomCompilerOptions,
    _codegen: &CodegenOptions,
    _custom_elements: &CustomElementMatcher,
    template_syntax: TemplateSyntaxMode,
    has_croquis: bool,
    s2_emit_selection: S2EmitSelection,
) -> bool {
    matches!(
        s2_emit_selection,
        S2EmitSelection::Allowed | S2EmitSelection::RequireSections
    ) && !options.ssr
        && !options.experimental_patterned_template
        && !options.custom_renderer
        && options.dialect == vize_s0::config::VueVersion::V3
        && template_syntax == TemplateSyntaxMode::Standard
        && !has_croquis
}

/// The published DOM option surface projected onto the S2 emitter.
///
/// Keep this conversion beside the legacy parse/transform wiring: the public
/// compiler still returns its AST and diagnostics, while S2 owns the supported
/// traversal surface. Source-map requests attach a verified compatibility map
/// in `compile::source_map`; a field missing here must stay on the compatibility
/// path rather than becoming an accidental S2 default.
pub(super) fn s2_emit_options<'a>(
    options: &'a DomCompilerOptions,
    codegen: &'a CodegenOptions,
    custom_elements: &'a CustomElementMatcher,
    bindings: Option<&'a BindingTable>,
    hoisted_scope_id: Option<&'a str>,
) -> Option<DomEmitOptions<'a>> {
    Some(DomEmitOptions {
        mode: match options.mode {
            CodegenMode::Function => DomEmitMode::Function,
            CodegenMode::Module => DomEmitMode::Module,
        },
        runtime_module_name: codegen.runtime_module_name.as_str(),
        runtime_global_name: codegen.runtime_global_name.as_str(),
        prefix_identifiers: options.prefix_identifiers,
        hoist_static: options.hoist_static,
        inline: options.inline,
        component_name: options.component_name.as_deref(),
        cache_handlers: options.cache_handlers,
        hoisted_scope_id,
        scope_id: options.scope_id.as_deref(),
        is_ts: options.is_ts,
        comments: options.comments,
        experimental_in_tag_comments: options.experimental_in_tag_comments,
        custom_element_patterns: custom_elements.patterns(),
        custom_element_predicate: custom_elements.static_predicate(),
        bindings,
    })
}

pub(super) fn s2_binding_table(metadata: Option<&BindingMetadata>) -> Option<BindingTable> {
    metadata.map(|metadata| {
        BindingTable::new(
            metadata
                .bindings
                .iter()
                .map(|(name, kind)| (name.as_str(), s2_binding_kind(*kind))),
            metadata
                .props_aliases
                .iter()
                .map(|(local, key)| (local.as_str(), key.as_str())),
            metadata.is_script_setup,
        )
    })
}

/// Emit one ordinary DOM module through S2.
pub(super) fn emit_s2(
    allocator: &Allocator,
    source: &str,
    dialect: vize_s0::config::VueVersion,
    options: &DomEmitOptions<'_>,
    pre_s2_walks: Option<WalkCounts>,
) -> Result<CodegenResultWithSections, EmitError> {
    let caps = LegacyCaps::for_version(dialect);
    let profiler = global_profiler();
    let emit = if profiler.is_enabled() {
        let observed =
            vize_s1_to_s2::emit_dom_source_observed_with_options(allocator, source, caps, options)?;
        let budget = observed.budget;
        // P2-12b observes the compiler path that actually produced this DOM
        // module. The regular entry point keeps the observer uninstantiated,
        // preserving the no-observer cost law outside explicit profiling.
        profiler.record_counter_enabled("davinci.s2_dom.files", 1);
        profiler.record_counter_enabled(
            "davinci.s2_dom.transform.walks",
            u64::from(budget.transform.walks),
        );
        profiler.record_counter_enabled(
            "davinci.s2_dom.transform.passes",
            u64::from(budget.transform.passes),
        );
        profiler.record_counter_enabled("davinci.s2_dom.emit.walks", u64::from(budget.emit_walks));
        profiler
            .record_counter_enabled("davinci.s2_dom.emit.visits", u64::from(budget.emit_visits));
        profiler.record_counter_enabled(
            "davinci.s2_dom.total.walks",
            u64::from(budget.total_walks()),
        );
        if let Some(pre_s2) = pre_s2_walks {
            let pre_s2_walks = pre_s2.total_walks();
            profiler.record_counter_enabled("davinci.s2_dom.pre_s2.walks", pre_s2_walks);
            profiler.record_counter_enabled("davinci.s2_dom.pre_s2.visits", pre_s2.total_visits());
            profiler.record_counter_enabled(
                "davinci.s2_dom.build.walks",
                pre_s2_walks + u64::from(budget.total_walks()),
            );
        }
        observed.emit
    } else {
        vize_s1_to_s2::emit_dom_source_with_options(allocator, source, caps, options)?
    };
    Ok(CodegenResultWithSections {
        result: CodegenResult {
            code: emit.code,
            preamble: emit.preamble,
            map: None,
        },
        // S2 records the same structural render-module boundaries as the
        // shipped emitter so SFC assembly can slice either lane identically.
        sections: Some(s2_codegen_sections(emit.sections)),
    })
}

const fn s2_codegen_sections(sections: DomEmitSections) -> CodegenSections {
    CodegenSections {
        imports_len: sections.imports_len,
        assets_start: sections.assets_start,
        assets_end: sections.assets_end,
        return_expr_start: sections.return_expr_start,
        return_expr_end: sections.return_expr_end,
    }
}

const fn s2_binding_kind(kind: BindingType) -> BindingKind {
    match kind {
        BindingType::SetupLet => BindingKind::SetupLet,
        BindingType::SetupMaybeRef => BindingKind::SetupMaybeRef,
        BindingType::SetupRef => BindingKind::SetupRef,
        BindingType::SetupReactiveConst => BindingKind::SetupReactiveConst,
        BindingType::SetupConst => BindingKind::SetupConst,
        BindingType::Props => BindingKind::Props,
        BindingType::PropsAliased => BindingKind::PropsAliased,
        BindingType::Data => BindingKind::Data,
        BindingType::Options => BindingKind::Options,
        BindingType::LiteralConst => BindingKind::LiteralConst,
        BindingType::JsGlobalUniversal => BindingKind::JsGlobalUniversal,
        BindingType::JsGlobalBrowser => BindingKind::JsGlobalBrowser,
        BindingType::JsGlobalNode => BindingKind::JsGlobalNode,
        BindingType::JsGlobalDeno => BindingKind::JsGlobalDeno,
        BindingType::JsGlobalBun => BindingKind::JsGlobalBun,
        BindingType::VueGlobal => BindingKind::VueGlobal,
        BindingType::ExternalModule => BindingKind::ExternalModule,
    }
}

#[cfg(test)]
mod tests;
