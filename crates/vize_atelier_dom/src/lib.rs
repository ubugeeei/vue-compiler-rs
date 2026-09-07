//! Vue compiler for DOM platform.
//!
//! This module provides DOM-specific compilation including:
//! - DOM element and attribute validation
//! - v-model transforms for form elements
//! - v-on event modifiers
//! - v-show transform
//! - Style and class binding handling

#![allow(clippy::collapsible_match)]
#![cfg_attr(
    test,
    allow(clippy::disallowed_macros, clippy::field_reassign_with_default)
)]

mod compile;
#[cfg(test)]
mod experimental_tests;
mod namespace;
pub mod options;
#[cfg(test)]
mod prefix_identifier_tests;
pub mod steps;

#[cfg(test)]
mod tests;

pub use compile::custom_elements::{
    compile_sfc_template_with_custom_elements_and_template_syntax_and_hoisted_scope_id_with_sections_and_codegen_options,
    compile_template_with_custom_elements_and_template_syntax_and_codegen_options,
    compile_template_with_custom_elements_and_template_syntax_and_hoisted_scope_id_and_codegen_options,
    compile_template_with_custom_elements_and_template_syntax_and_hoisted_scope_id_with_sections_and_codegen_options,
};
/// The differential lanes' old side — see its own docs for why the ordinary
/// entry points cannot serve as one.
#[cfg(feature = "davinci-differential")]
pub use compile::legacy::compile_template_legacy_with_options;
pub use compile::{
    compile_template, compile_template_with_options,
    compile_template_with_options_and_hoisted_scope_id, compile_template_with_template_syntax,
    compile_template_with_template_syntax_and_codegen_options,
    compile_template_with_template_syntax_and_hoisted_scope_id,
    compile_template_with_template_syntax_and_hoisted_scope_id_with_sections,
    compile_template_with_template_syntax_and_hoisted_scope_id_with_sections_and_codegen_options,
};
#[allow(deprecated)]
pub use compile::{
    compile_template_with_vue_parser_quirks,
    compile_template_with_vue_parser_quirks_and_hoisted_scope_id,
};
pub use options::{DomCompilerOptions, element_checks, event_modifiers};
pub use steps::{
    EventModifiers, EventOptions, MouseModifiers, PropagationModifiers, SystemModifiers, V_SHOW,
    V_TEXT, VModelModifiers, generate_html_prop, generate_html_warning, generate_key_guard,
    generate_model_props, generate_modifier_guard, generate_show_directive, generate_show_style,
    generate_text_children, generate_text_content, get_model_event, get_model_helper,
    get_model_prop, is_v_html, is_v_show, is_v_text, resolve_key_alias,
};

// Re-export core types
pub use vize_atelier_core::{
    Allocator, CompilerError, ElementNode, Namespace, RootNode, TemplateChildNode, codegen, errors,
    lane, parser, runtime_helpers, tokenizer, transform,
};
