//! The per-file binding tables that decide whether a template expression is
//! emitted as a typed check or as a bare `void (...)`.

use super::component_ref_callbacks::{
    ComponentRefCallbackBindings, collect_component_ref_callback_bindings,
};
use super::directive_values::{DirectiveValueBindings, collect_directive_value_bindings};
use super::native_props::{NativePropBindings, collect_native_prop_bindings};
use crate::virtual_ts::scope::ScopeGenerationOptions;
use vize_croquis::Croquis;

/// The per-file binding tables that turn a plain template expression into a
/// typed check rather than a bare `void (...)`: native element props and custom
/// directive values. Both are keyed by the expression's authored range, and an
/// expression present in neither is emitted unchecked.
///
/// Owned by [`TemplateValueCheckTables`] for one call to scope-closure
/// generation and borrowed from there down the emit tree.
#[derive(Clone, Copy)]
pub(crate) struct TemplateValueChecks<'a> {
    pub(crate) native_props: &'a NativePropBindings,
    pub(crate) directive_values: &'a DirectiveValueBindings,
    pub(crate) component_ref_callbacks: &'a ComponentRefCallbackBindings,
}

/// Owning form of [`TemplateValueChecks`], collected once per file.
///
/// The two tables are gathered together because the conditions that enable them
/// belong next to the checks they gate rather than at the call site: native prop
/// checks follow `check_props`, directive value checks follow
/// `check_template_bindings`, and neither runs under legacy Vue 2 output, whose
/// generated modules do not carry the Vue 3 types either check resolves through.
pub(crate) struct TemplateValueCheckTables {
    native_props: NativePropBindings,
    directive_values: DirectiveValueBindings,
    component_ref_callbacks: ComponentRefCallbackBindings,
}

impl TemplateValueCheckTables {
    pub(crate) fn collect(summary: &Croquis, options: &ScopeGenerationOptions<'_, '_>) -> Self {
        let legacy_vue2 = options.legacy_vue2;
        Self {
            native_props: collect_native_prop_bindings(
                options.template_ast,
                options.check_options.check_props && !legacy_vue2,
            ),
            directive_values: collect_directive_value_bindings(
                options.template_ast,
                &summary.bindings,
                options.check_options.check_template_bindings && !legacy_vue2,
            ),
            component_ref_callbacks: collect_component_ref_callback_bindings(
                options.template_ast,
                options.check_options.check_template_bindings && !legacy_vue2,
            ),
        }
    }

    pub(crate) fn as_checks(&self) -> TemplateValueChecks<'_> {
        TemplateValueChecks {
            native_props: &self.native_props,
            directive_values: &self.directive_values,
            component_ref_callbacks: &self.component_ref_callbacks,
        }
    }
}
