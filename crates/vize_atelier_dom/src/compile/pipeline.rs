use vize_atelier_core::options::{CodegenOptions, CustomElementMatcher};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum S2EmitSelection {
    Allowed,
    RequireSections,
    Disabled,
}

pub(super) struct DomCompilePipelineOptions {
    pub(super) custom_elements: CustomElementMatcher,
    pub(super) codegen_options: CodegenOptions,
    pub(super) s2_emit_selection: S2EmitSelection,
}

impl DomCompilePipelineOptions {
    pub(super) fn allow_s2(
        custom_elements: CustomElementMatcher,
        codegen_options: CodegenOptions,
    ) -> Self {
        Self {
            custom_elements,
            codegen_options,
            s2_emit_selection: S2EmitSelection::Allowed,
        }
    }

    pub(super) fn require_sections(
        custom_elements: CustomElementMatcher,
        codegen_options: CodegenOptions,
    ) -> Self {
        Self {
            custom_elements,
            codegen_options,
            s2_emit_selection: S2EmitSelection::RequireSections,
        }
    }

    /// The legacy lane, S2 declined — the differential lanes' old side.
    #[cfg(feature = "davinci-differential")]
    pub(super) fn deny_s2(
        custom_elements: CustomElementMatcher,
        codegen_options: CodegenOptions,
    ) -> Self {
        Self {
            custom_elements,
            codegen_options,
            s2_emit_selection: S2EmitSelection::Disabled,
        }
    }

    pub(super) fn require_sections_compat(
        custom_elements: CustomElementMatcher,
        codegen_options: CodegenOptions,
    ) -> Self {
        Self {
            custom_elements,
            codegen_options,
            s2_emit_selection: S2EmitSelection::Disabled,
        }
    }
}
