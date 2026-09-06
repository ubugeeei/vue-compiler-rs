//! Compiling lowered JSX/TSX into Vue VDOM render output.
//!
//! This reuses the existing Vize DOM compiler infrastructure rather than a
//! separate Babel-style emitter: the shared lowering layer produces a
//! [`RootNode`](vize_relief::RootNode), which is fed straight into
//! `vize_atelier_core`'s transform lane and codegen entry point — the same
//! route the SFC template path uses.
//!
//! Unlike SFC templates, JSX render functions are real closures over the
//! component's setup scope, so identifier expressions are **not** prefixed with
//! `_ctx.`. Static hoisting and handler caching default off for predictable,
//! `@vue/babel-plugin-jsx`-shaped output; callers can opt in.

mod s2_emit;

use vize_atelier_core::codegen::generate_with_vnode_factory_and_merge_props;
use vize_atelier_core::lane::transform_with_jsx_compatibility;
use vize_atelier_core::options::{CodegenMode, CodegenOptions, TransformOptions};
// `CodegenMode::Module` is the only supported JSX target: JSX/TSX is authored
// for bundlers, and the runtime `Function` (with-block) mode emits an empty
// body under JSX's no-prefix closure model.
use vize_atelier_core::CompilerError;
use vize_croquis::Croquis;
use vize_s0::{Allocator, String};

use crate::diagnostics::JsxDiagnostic;
use crate::scoped::{ScopedStyle, build_scoped_style};
use crate::{ComponentSetupSpan, JsxLang, JsxOutputMode, LoweredRoot, lower_source};

/// Options controlling JSX/TSX -> VDOM compilation.
///
/// Defaults keep `@vue/babel-plugin-jsx`-shaped output: no static hoisting, no
/// handler caching, no source map.
#[derive(Debug, Clone, Default)]
pub struct VdomCompileOptions {
    /// Hoist static subtrees out of the render function.
    pub hoist_static: bool,
    /// Cache inline event handlers.
    pub cache_handlers: bool,
    /// Emit a source map.
    pub source_map: bool,
}

/// One compiled component render expression.
pub struct VdomComponent {
    /// Enclosing component-function name, if resolved.
    pub component_name: Option<String>,
    /// Source spans for rebuilding block-body JSX components as stateful Vue
    /// components.
    pub component_setup: Option<ComponentSetupSpan>,
    /// Resolved output mode (defaults to [`JsxOutputMode::Vdom`]).
    pub mode: JsxOutputMode,
    /// Generated render code.
    pub code: String,
    /// Import/preamble section for runtime helpers.
    pub preamble: String,
    /// v3 source map (JSON) mapping the generated render code back to the JSX
    /// source, emitted only when [`VdomCompileOptions::source_map`] is set
    /// (#1533). `None` otherwise. The map's `mappings` cover the render
    /// expression; it does not account for a prepended preamble, so a consumer
    /// that inlines the preamble must offset accordingly (the bindings surface
    /// the map alongside a `preamble` kept structurally separate for exactly
    /// this reason).
    pub map: Option<String>,
    /// Extracted `<style scoped>` block (#1495): the generated scope id and the
    /// scoped-rewritten CSS. `None` when the component had no `<style scoped>`.
    /// A bundler emits this CSS to a stylesheet (deferred, #1533); the scope id
    /// is already injected into the render output's elements.
    pub scoped_style: Option<ScopedStyle>,
}

/// Result of compiling a JSX/TSX module to VDOM.
pub struct VdomOutput {
    /// One entry per outermost JSX render root, in source order.
    pub components: Vec<VdomComponent>,
    /// Parse, lowering, and transform diagnostics.
    pub diagnostics: Vec<JsxDiagnostic>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VdomCompatOptions<'a> {
    pub transform_on_helper: Option<&'a str>,
    pub object_slots_helpers: Option<(&'a str, &'a str)>,
    pub vnode_factory: Option<&'a str>,
    pub merge_props: bool,
    pub allow_static_v_model_arg_on_element: bool,
    pub custom_element_spans: &'a [(u32, u32)],
}

impl Default for VdomCompatOptions<'_> {
    fn default() -> Self {
        Self {
            transform_on_helper: None,
            object_slots_helpers: None,
            vnode_factory: None,
            merge_props: true,
            allow_static_v_model_arg_on_element: false,
            custom_element_spans: &[],
        }
    }
}

impl VdomOutput {
    /// Whether any error-severity diagnostic was produced.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(JsxDiagnostic::is_error)
    }
}

/// Compile a JSX/TSX module into Vue VDOM render functions.
pub fn compile_to_vdom(
    allocator: &Allocator,
    source: &str,
    lang: JsxLang,
    options: VdomCompileOptions,
) -> VdomOutput {
    let lowered = lower_source(allocator, allocator.as_oxc(), source, lang);
    let mut diagnostics = lowered.diagnostics;
    let is_ts = lang.is_typescript();

    // Park the analysis on the allocator so the transform can borrow it for `'a`.
    let analysis: &Croquis = allocator.alloc_owned(lowered.analysis);

    let mut components = Vec::with_capacity(lowered.roots.len());
    for lowered_root in lowered.roots {
        components.push(compile_root_to_vdom(
            allocator,
            lowered_root,
            analysis,
            is_ts,
            &options,
            VdomCompatOptions::default(),
            &mut diagnostics,
            source,
        ));
    }

    VdomOutput {
        components,
        diagnostics,
    }
}

/// Compile a single already-lowered root to a VDOM [`VdomComponent`], appending
/// any transform diagnostics. Shared by [`compile_to_vdom`] and the mode-aware
/// dispatcher in [`crate::compile`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_root_to_vdom(
    allocator: &Allocator,
    lowered: LoweredRoot,
    analysis: &Croquis,
    is_ts: bool,
    options: &VdomCompileOptions,
    compat: VdomCompatOptions<'_>,
    diagnostics: &mut Vec<JsxDiagnostic>,
    source: &str,
) -> VdomComponent {
    let LoweredRoot {
        mut root,
        s2,
        mode,
        component_name,
        component_setup,
        scoped_css,
        // The style interpolation spans are consumed by the type checker
        // (`vize_canon`), not the VDOM scoping backend.
        scoped_style_exprs: _,
    } = lowered;

    // Extract + rewrite the `<style scoped>` CSS and derive the scope id, reusing
    // the SFC scope infrastructure. The id is injected into rendered elements by
    // the codegen via `CodegenOptions.scope_id` below.
    let scoped_style =
        scoped_css.map(|css| build_scoped_style(component_name.as_deref(), css.as_str()));

    if let Some(emit) = s2_emit::try_emit_s2_vdom(
        allocator,
        s2,
        is_ts,
        component_name.as_deref(),
        scoped_style.as_ref().map(|style| style.scope_id.as_str()),
        options,
        &compat,
    ) {
        return VdomComponent {
            component_name,
            component_setup,
            mode: mode.unwrap_or(JsxOutputMode::Vdom),
            code: emit.code,
            preamble: emit.preamble,
            map: None,
            scoped_style,
        };
    }

    let transform_opts = TransformOptions {
        // JSX render fns close over the setup scope; don't prefix `_ctx.`.
        prefix_identifiers: false,
        hoist_static: options.hoist_static,
        cache_handlers: options.cache_handlers,
        is_ts,
        // Binding info is supplied via the `analysis` Croquis below; the
        // relief-side `binding_metadata` (a distinct type) is only needed
        // for SFC inline-mode ref unwrapping, which JSX closures don't use.
        binding_metadata: None,
        ..Default::default()
    };
    let errors = transform_with_jsx_compatibility(
        allocator,
        &mut root,
        transform_opts,
        Some(analysis),
        compat.allow_static_v_model_arg_on_element,
        compat.custom_element_spans,
        Some(source),
    );
    diagnostics.extend(errors.iter().map(compiler_error_to_diagnostic));

    let codegen_opts = CodegenOptions {
        mode: CodegenMode::Module,
        source_map: options.source_map,
        component_name: component_name.clone(),
        is_ts,
        cache_handlers: options.cache_handlers,
        binding_metadata: None,
        // Inject the `data-v-<hash>` scope attribute into every rendered element
        // (the same codegen path SFC scoped styles use).
        scope_id: scoped_style.as_ref().map(|style| style.scope_id.clone()),
        ..Default::default()
    };
    let result = generate_with_vnode_factory_and_merge_props(
        &root,
        codegen_opts,
        compat.vnode_factory,
        compat.merge_props,
        Some(source),
    );
    let mut preamble = result.preamble;
    if let Some(helper) = compat.transform_on_helper
        && result.code.contains(helper)
    {
        if !preamble.is_empty() && !preamble.ends_with('\n') {
            preamble.push('\n');
        }
        preamble.push_str("import ");
        preamble.push_str(helper);
        preamble.push_str(" from \"@vue/babel-helper-vue-transform-on\"\n");
    }
    if let Some((is_slot, is_vnode)) = compat.object_slots_helpers
        && result.code.contains(is_slot)
    {
        if !preamble.is_empty() && !preamble.ends_with('\n') {
            preamble.push('\n');
        }
        preamble.push_str("import { isVNode as ");
        preamble.push_str(is_vnode);
        preamble.push_str(" } from \"vue\"\n");
        preamble.push_str("function ");
        preamble.push_str(is_slot);
        preamble.push_str("(s) {\n  return typeof s === 'function' || ");
        preamble.push_str("Object.prototype.toString.call(s) === '[object Object]' && !");
        preamble.push_str(is_vnode);
        preamble.push_str("(s);\n}\n");
    }

    VdomComponent {
        component_name,
        component_setup,
        mode: mode.unwrap_or(JsxOutputMode::Vdom),
        code: result.code,
        preamble,
        map: result.map,
        scoped_style,
    }
}

fn compiler_error_to_diagnostic(error: &CompilerError) -> JsxDiagnostic {
    let (start, end) = error
        .loc
        .as_ref()
        .map(|loc| (loc.span.start, loc.span.end))
        .unwrap_or((0, 0));
    JsxDiagnostic::error(error.message.as_str(), start, end)
}
