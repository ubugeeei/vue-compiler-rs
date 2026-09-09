mod auto_imports;
mod deferred_bindings;

use std::ops::Range;

use vize_croquis::{BindingType, Croquis};
use vize_s0::config::VueVersion;
use vize_s0::{FxHashMap, FxHashSet, String, append, cstr};

use super::super::types::{VirtualTsGenerationOptions, VirtualTsOptions};
use super::anchors::emit_props_shadow_anchor;
use super::legacy_vue2::{needs_legacy_vue2_helpers, ref_unwrap_helper_for_template};
use crate::virtual_ts::{VizeSemanticLink, VizeSemanticLinkKind};

pub(super) struct TemplateRefUnwraps {
    setup_bindings: Vec<String>,
    options_api_setup_bindings: Vec<String>,
    auto_import_bindings: Vec<String>,
    /// Setup bindings whose assignment is deferred past their declaration. See
    /// `deferred_bindings` for why they need the same shadowing treatment.
    deferred_bindings: Vec<String>,
    /// Setup bindings produced by `defineModel`. They need the model getter type
    /// in templates, while ordinary refs stay on the cheaper generic unwrap.
    model_ref_bindings: FxHashSet<String>,
    /// Dialect and preamble decisions resolved once at collection time; they
    /// also select the `__U` helper this shadow set is emitted against.
    legacy_helpers: bool,
    dialect: VueVersion,
    hoist_shared_preamble: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_and_emit_scope_preamble(
    ts: &mut String,
    summary: &Croquis,
    options_api: bool,
    template_referenced_names: &FxHashSet<String>,
    script_content: Option<&str>,
    imported_names: &FxHashSet<&str>,
    options: &VirtualTsOptions,
    generation_options: VirtualTsGenerationOptions<'_>,
    has_generic_param: bool,
    semantic_links: &mut Vec<VizeSemanticLink>,
) -> TemplateRefUnwraps {
    let unwraps = TemplateRefUnwraps::collect(
        summary,
        options_api,
        Some(template_referenced_names),
        script_content,
        imported_names,
        options,
        generation_options,
    );
    let captures = unwraps.emit_type_captures(ts);
    emit_props_shadow_anchor(ts, summary, template_referenced_names);
    // Semicolon prevents ASI issues when user script doesn't end with `;`
    // (e.g., `console.log(x)\n(function...)` would be parsed as a call)
    ts.push_str("  ;(function __template() {\n");
    unwraps.emit_template_variables(ts, has_generic_param, &captures, semantic_links);
    unwraps
}

impl TemplateRefUnwraps {
    pub(super) fn collect(
        summary: &Croquis,
        options_api: bool,
        template_referenced_names: Option<&FxHashSet<String>>,
        script_content: Option<&str>,
        imported_names: &FxHashSet<&str>,
        options: &VirtualTsOptions,
        generation_options: VirtualTsGenerationOptions<'_>,
    ) -> Self {
        let legacy_helpers =
            needs_legacy_vue2_helpers(generation_options.legacy_vue2, generation_options.dialect);
        let options_api_setup_bindings =
            crate::options_api_setup_spread::collect_template_setup_bindings(
                summary,
                options_api,
                template_referenced_names,
                script_content,
            );
        let options_api_setup_binding_names: FxHashSet<&str> = options_api_setup_bindings
            .iter()
            .map(|name| name.as_str())
            .collect();

        let mut setup_bindings: Vec<String> = summary
            .bindings
            .bindings
            .iter()
            .filter(|(name, _)| {
                template_referenced_names
                    .is_none_or(|referenced| referenced.contains(name.as_str()))
            })
            .filter(|(name, _)| !options_api_setup_binding_names.contains(name.as_str()))
            .filter(|(name, binding_type)| {
                summary.reactivity.needs_value_access(name.as_str())
                    || matches!(binding_type, BindingType::SetupMaybeRef)
            })
            .map(|(name, _)| String::from(name.as_str()))
            .collect();
        setup_bindings.sort_unstable();
        let model_ref_bindings = summary
            .macros
            .models()
            .iter()
            .map(|model| String::from(model.local_name.as_str()))
            .filter(|name| setup_bindings.iter().any(|binding| binding == name))
            .collect();

        let auto_import_bindings = template_referenced_names
            .map(|referenced| {
                auto_imports::collect(
                    summary,
                    options,
                    imported_names,
                    &options_api_setup_binding_names,
                    referenced,
                    legacy_helpers,
                )
            })
            .unwrap_or_default();

        // Shadowing a name twice would redeclare it with a conflicting type,
        // so the deferred set excludes everything already shadowed above.
        let deferred_bindings = deferred_bindings::collect_deferred_setup_bindings(
            summary,
            script_content,
            template_referenced_names,
            |name| {
                setup_bindings.iter().any(|shadowed| shadowed == name)
                    || auto_import_bindings.iter().any(|shadowed| shadowed == name)
                    || options_api_setup_binding_names.contains(name)
            },
        );

        Self {
            setup_bindings,
            options_api_setup_bindings,
            auto_import_bindings,
            deferred_bindings,
            model_ref_bindings,
            legacy_helpers,
            dialect: generation_options.dialect,
            hoist_shared_preamble: generation_options.hoist_shared_preamble,
        }
    }

    pub(super) fn setup_spread_bindings(&self) -> &[String] {
        &self.options_api_setup_bindings
    }

    pub(super) fn emit_type_captures(
        &self,
        mut ts: &mut String,
    ) -> FxHashMap<String, Range<usize>> {
        let mut captures = deferred_bindings::emit_type_captures(ts, &self.deferred_bindings);
        if !self.setup_bindings.is_empty() {
            ts.push_str("  // Ref type captures (before template scope shadows them)\n");
            for name in &self.setup_bindings {
                record_typeof_capture(ts, name, &mut captures, "__R");
            }
        }
        if !self.options_api_setup_bindings.is_empty() {
            ts.push_str(
                "  // Options API setup return type captures (before template scope shadows them)\n",
            );
            ts.push_str(
                "  type __VizeOptionsSetupBinding<K extends string> = typeof __default__ extends abstract new (...args: any) => infer __I ? K extends keyof __I ? __I[K] : any : any;\n",
            );
            for name in &self.options_api_setup_bindings {
                append!(
                    ts,
                    "  type __R_{name} = __VizeOptionsSetupBinding<\"{name}\">;\n"
                );
            }
        }
        if !self.auto_import_bindings.is_empty() {
            ts.push_str(
                "  // Auto-import ref type captures (before template scope shadows them)\n",
            );
            for name in &self.auto_import_bindings {
                record_typeof_capture(ts, name, &mut captures, "__R");
            }
        }
        captures
    }

    /// Shadow ref bindings with their unwrapped types. `var` allows
    /// reassignment, which Vue templates do to refs.
    ///
    /// Emits nothing at all when template scope has no setup binding to
    /// shadow, which is why the conditional types `__U` delegates to are
    /// declared alongside it rather than at module scope — see
    /// `legacy_vue2::MODERN_REF_UNWRAP_HELPER`.
    pub(super) fn emit_template_variables(
        &self,
        mut ts: &mut String,
        has_generic_param: bool,
        captures: &FxHashMap<String, Range<usize>>,
        semantic_links: &mut Vec<VizeSemanticLink>,
    ) {
        deferred_bindings::emit_template_variables(
            ts,
            &self.deferred_bindings,
            captures,
            semantic_links,
        );
        if self.setup_bindings.is_empty()
            && self.options_api_setup_bindings.is_empty()
            && self.auto_import_bindings.is_empty()
        {
            return;
        }

        ts.push_str("    // Auto-unwrap Vue refs in template scope\n");
        ts.push_str(ref_unwrap_helper_for_template(
            self.legacy_helpers,
            self.dialect,
            has_generic_param,
            self.hoist_shared_preamble,
        ));
        if !self.model_ref_bindings.is_empty() {
            ts.push_str(model_ref_unwrap_helper_for_template(
                self.legacy_helpers,
                self.dialect,
                has_generic_param,
            ));
        }
        for name in &self.setup_bindings {
            let helper = if self.model_ref_bindings.contains(name) {
                "__M<__R_"
            } else {
                "__U<__R_"
            };
            record_template_shadow(ts, name, helper, ">", captures, semantic_links);
        }
        for name in &self.options_api_setup_bindings {
            append!(ts, "    var {name}: __U<__R_{name}> = undefined as any;\n");
        }
        for name in &self.auto_import_bindings {
            record_template_shadow(ts, name, "__U<__R_", ">", captures, semantic_links);
        }
    }
}

fn record_typeof_capture(
    ts: &mut String,
    name: &str,
    captures: &mut FxHashMap<String, Range<usize>>,
    helper_prefix: &str,
) {
    let line = cstr!("  type {helper_prefix}_{name} = typeof {name};\n");
    let start = ts.len()
        + line
            .rfind(name)
            .expect("capture line should contain binding name");
    ts.push_str(line.as_str());
    captures.insert(String::from(name), start..start + name.len());
}

fn record_template_shadow(
    ts: &mut String,
    name: &str,
    type_prefix: &str,
    type_suffix: &str,
    captures: &FxHashMap<String, Range<usize>>,
    semantic_links: &mut Vec<VizeSemanticLink>,
) {
    let line = cstr!("    var {name}: {type_prefix}{name}{type_suffix} = undefined as any;\n");
    let start = ts.len()
        + line
            .find(name)
            .expect("template shadow line should contain binding name");
    ts.push_str(line.as_str());
    if let Some(source_range) = captures.get(name) {
        semantic_links.push(VizeSemanticLink {
            source_range: source_range.clone(),
            target_range: start..start + name.len(),
            kind: VizeSemanticLinkKind::VueSetupTemplateRefUnwrap,
        });
    }
}

fn model_ref_unwrap_helper_for_template(
    legacy_helpers: bool,
    _dialect: VueVersion,
    has_generic_param: bool,
) -> &'static str {
    if legacy_helpers || has_generic_param {
        "    type __M<T> = T extends __VizeModelRef<any, any, infer __G, any> ? __G : __U<T>;\n"
    } else {
        "    type __M<T> = T extends __VizeModelRef<any, any, infer __G, any> ? __VizeWidenTemplateRef<__G> : __U<T>;\n"
    }
}
