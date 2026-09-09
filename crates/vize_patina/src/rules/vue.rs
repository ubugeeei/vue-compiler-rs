//! Vue-specific lint rules.
//!
//! These rules are compatible with eslint-plugin-vue's essential and
//! strongly-recommended rule sets.
//!
//! ## Rule Categories
//!
//! - **Essential**: Prevent errors (default severity: Error)
//! - **Strongly Recommended**: Improve readability (default severity: Warning)
//! - **Recommended**: Ensure consistency (default severity: Warning)

// Essential rules
mod attribute_hyphenation;
mod attribute_order;
mod no_bare_strings_in_template;
mod no_child_content;
mod no_deprecated_filter;
mod no_deprecated_functional_template;
mod no_deprecated_html_element_is;
mod no_deprecated_inline_template;
mod no_deprecated_router_link_tag_prop;
mod no_deprecated_scope_attribute;
mod no_deprecated_slot_attribute;
mod no_deprecated_slot_scope_attribute;
mod no_deprecated_v_bind_sync;
mod no_deprecated_v_on_native_modifier;
mod no_deprecated_v_on_number_modifiers;
mod no_dupe_v_else_if;
mod no_duplicate_attributes;
mod no_invalid_html_attribute;
mod no_reserved_component_names;
mod no_static_inline_styles;
mod no_template_key;
mod no_textarea_mustache;
mod no_unused_vars;
mod no_use_v_else_with_v_for;
mod no_use_v_if_with_v_for;
mod no_useless_template_attributes;
mod no_v_for_template_key_on_child;
mod no_v_text_v_html_on_component;
mod prop_name_casing;
mod require_component_is;
mod require_scoped_style;
mod require_toggle_inside_transition;
mod require_v_for_key;
mod valid_attribute_name;
mod valid_template_root;
mod valid_v_bind;
mod valid_v_cloak;
mod valid_v_else;
mod valid_v_for;
mod valid_v_html;
mod valid_v_if;
mod valid_v_memo;
mod valid_v_model;
mod valid_v_on;
mod valid_v_once;
mod valid_v_show;
mod valid_v_slot;
mod valid_v_text;

// Strongly recommended rules
mod component_definition_name_casing;
mod html_quotes;
mod mustache_interpolation_spacing;
mod no_lone_template;
mod no_multi_spaces;
mod no_multiple_template_root;
mod no_non_component_keep_alive_child;
mod sfc_element_order;
mod v_on_style;
mod v_slot_style;
// Most implementations live under rules::opinionated::vue and are re-exported here.

// Security rules
mod no_template_target_blank;
mod no_unsafe_url;
mod no_unsandboxed_iframe;
mod no_v_html;

// Semantic analysis rules (require Croquis)
mod no_mutating_props;
mod no_undefined_refs;
mod no_unused_components;
mod no_unused_properties;

// Accessibility rules
mod a11y_img_alt;
mod permitted_contents;
// `use_unique_element_ids` implementation lives under rules::opinionated::vue.

// Style rules
mod single_style_block;
// Opinionated style rules live under rules::opinionated::vue.

// Warning rules
// Opinionated warning rules live under rules::opinionated::vue.

// Essential rules exports
pub use crate::rules::opinionated::vue::MultiWordComponentNames;
pub use crate::rules::opinionated::vue::UseVOnExact;
pub use no_bare_strings_in_template::NoBareStringsInTemplate;
pub use no_child_content::NoChildContent;
pub use no_deprecated_filter::NoDeprecatedFilter;
pub use no_deprecated_functional_template::NoDeprecatedFunctionalTemplate;
pub use no_deprecated_html_element_is::NoDeprecatedHtmlElementIs;
pub use no_deprecated_inline_template::NoDeprecatedInlineTemplate;
pub use no_deprecated_router_link_tag_prop::NoDeprecatedRouterLinkTagProp;
pub use no_deprecated_scope_attribute::NoDeprecatedScopeAttribute;
pub use no_deprecated_slot_attribute::NoDeprecatedSlotAttribute;
pub use no_deprecated_slot_scope_attribute::NoDeprecatedSlotScopeAttribute;
pub use no_deprecated_v_bind_sync::NoDeprecatedVBindSync;
pub use no_deprecated_v_on_native_modifier::NoDeprecatedVOnNativeModifier;
pub use no_deprecated_v_on_number_modifiers::NoDeprecatedVOnNumberModifiers;
pub use no_dupe_v_else_if::NoDupeVElseIf;
pub use no_duplicate_attributes::NoDuplicateAttributes;
pub use no_invalid_html_attribute::NoInvalidHtmlAttribute;
pub use no_reserved_component_names::NoReservedComponentNames;
pub use no_template_key::NoTemplateKey;
pub use no_textarea_mustache::NoTextareaMustache;
pub use no_unused_vars::NoUnusedVars;
pub use no_use_v_else_with_v_for::NoUseVElseWithVFor;
pub use no_use_v_if_with_v_for::NoUseVIfWithVFor;
pub use no_useless_template_attributes::NoUselessTemplateAttributes;
pub use no_v_for_template_key_on_child::NoVForTemplateKeyOnChild;
pub use no_v_text_v_html_on_component::NoVTextVHtmlOnComponent;
pub use require_component_is::RequireComponentIs;
pub use require_toggle_inside_transition::RequireToggleInsideTransition;
pub use require_v_for_key::RequireVForKey;
pub use valid_attribute_name::ValidAttributeName;
pub use valid_template_root::ValidTemplateRoot;
pub use valid_v_bind::ValidVBind;
pub use valid_v_cloak::ValidVCloak;
pub use valid_v_else::ValidVElse;
pub use valid_v_for::ValidVFor;
pub use valid_v_html::ValidVHtml;
pub use valid_v_if::ValidVIf;
pub use valid_v_memo::ValidVMemo;
pub use valid_v_model::ValidVModel;
pub use valid_v_on::ValidVOn;
pub use valid_v_once::ValidVOnce;
pub use valid_v_show::ValidVShow;
pub use valid_v_slot::ValidVSlot;
pub use valid_v_text::ValidVText;

// Strongly recommended rules exports
pub use crate::rules::opinionated::vue::NoTemplateShadow;
pub use crate::rules::opinionated::vue::{
    HtmlSelfClosing, HtmlSelfClosingHtmlOptions, HtmlSelfClosingOptions, HtmlSelfClosingStyle,
};
pub use crate::rules::opinionated::vue::{VBindStyle, VBindStyleOption};
pub use attribute_hyphenation::AttributeHyphenation;
pub use component_definition_name_casing::ComponentDefinitionNameCasing;
pub use html_quotes::{HtmlQuotes, HtmlQuotesOption};
pub use mustache_interpolation_spacing::MustacheInterpolationSpacing;
pub use no_multi_spaces::NoMultiSpaces;
pub use no_multiple_template_root::NoMultipleTemplateRoot;
pub use no_static_inline_styles::NoStaticInlineStyles;
pub use prop_name_casing::PropNameCasing;
pub use v_on_style::{VOnStyle, VOnStyleOption};
pub use v_slot_style::VSlotStyle;

// Recommended rules exports
pub use crate::rules::opinionated::vue::ComponentNameInTemplateCasing;
pub use crate::rules::opinionated::vue::NoInlineStyle;
pub use crate::rules::opinionated::vue::PreferPropsShorthand;
pub use crate::rules::opinionated::vue::RequireComponentRegistration;
pub use crate::rules::opinionated::vue::ScopedEventNames;
pub use attribute_order::AttributeOrder;
pub use no_lone_template::NoLoneTemplate;
pub use no_non_component_keep_alive_child::NoNonComponentKeepAliveChild;
pub use sfc_element_order::SfcElementOrder;

// Security rules exports
pub use no_template_target_blank::NoTemplateTargetBlank;
pub use no_unsafe_url::NoUnsafeUrl;
pub use no_unsandboxed_iframe::NoUnsandboxedIframe;
pub use no_v_html::NoVHtml;

// Semantic analysis rules exports
pub use no_mutating_props::NoMutatingProps;
pub use no_undefined_refs::NoUndefinedRefs;
pub use no_unused_components::NoUnusedComponents;
pub use no_unused_properties::NoUnusedProperties;

// Accessibility rules exports
pub use crate::rules::opinionated::vue::UseUniqueElementIds;
pub use a11y_img_alt::A11yImgAlt;
pub use permitted_contents::PermittedContents;

// HTML conformance rules exports
pub use crate::rules::opinionated::vue::NoBooleanAttrValue;

// Style rules exports
pub use crate::rules::opinionated::vue::NoPreprocessorLang;
pub use crate::rules::opinionated::vue::NoScriptNonStandardLang;
pub use crate::rules::opinionated::vue::NoSrcAttribute;
pub use crate::rules::opinionated::vue::NoTemplateLang;
pub use require_scoped_style::RequireScopedStyle;
pub use single_style_block::SingleStyleBlock;

// Warning rules exports
pub use crate::rules::opinionated::vue::WarnCustomBlock;
pub use crate::rules::opinionated::vue::WarnCustomDirective;

/// Register the shared `v-*` directive correctness rules every preset enables.
///
/// Most of these check that built-in directives carry the expected expression,
/// argument, and modifier shape (e.g. `v-cloak` takes none, `v-text` takes
/// one). The bundle also folds in a few essential markup checks of the same
/// class — `v-for` key placement, `<transition>` requiring a toggle on its
/// wrapped element, and a valid single template root — that belong with the
/// other essential directive-correctness rules. Bundling them here keeps the
/// per-preset registration list short.
pub(crate) fn register_valid_directives(registry: &mut crate::rule::RuleRegistry) {
    registry.register(Box::new(ValidVMemo));
    registry.register(Box::new(ValidVCloak));
    registry.register(Box::new(ValidVOnce));
    registry.register(Box::new(ValidVText));
    registry.register(Box::new(ValidVHtml));
    registry.register(Box::new(NoVForTemplateKeyOnChild));
    registry.register(Box::new(RequireToggleInsideTransition));
    registry.register(Box::new(ValidTemplateRoot));
}

/// Register the default-preset security rules in one call.
///
/// These flag template-shaped attack surface (raw `v-html`, unsafe URL
/// schemes, untrusted `target="_blank"`, unsandboxed iframes, and statically
/// invalid HTML attributes). Bundling them here keeps the per-preset
/// registration list short.
pub(crate) fn register_security(registry: &mut crate::rule::RuleRegistry) {
    registry.register(Box::new(NoVHtml));
    registry.register(Box::new(NoUnsafeUrl));
    registry.register(Box::new(NoTemplateTargetBlank));
    registry.register(Box::new(NoUnsandboxedIframe));
    registry.register(Box::new(NoInvalidHtmlAttribute));
}

/// Register Vue rules that require explicit opt-in.
///
/// This includes contextual contracts such as Nuxt's single-root pages,
/// migration checks for deprecated Vue 2 syntax, and documented semantic
/// checks that are not in the default preset. Keeping them outside every
/// preset avoids imposing those narrower contracts on general Vue 3 projects.
/// A documented rule still has to be instantiable here: config-enabling a
/// name that no registry entry owns is a silent false negative (#4636).
pub(crate) fn register_opt_in(registry: &mut crate::rule::RuleRegistry) {
    if !registry.has_rule("vue/a11y-img-alt") {
        registry.register(Box::new(A11yImgAlt));
    }
    if !registry.has_rule("vue/no-undefined-refs") {
        registry.register(Box::new(NoUndefinedRefs));
    }
    if !registry.has_rule("vue/no-multiple-template-root") {
        registry.register(Box::new(NoMultipleTemplateRoot));
    }
    if !registry.has_rule("vue/no-use-v-else-with-v-for") {
        registry.register(Box::new(NoUseVElseWithVFor));
    }
    if !registry.has_rule("vue/no-deprecated-v-on-native-modifier") {
        registry.register(Box::new(NoDeprecatedVOnNativeModifier));
    }
    if !registry.has_rule("vue/no-deprecated-v-on-number-modifiers") {
        registry.register(Box::new(NoDeprecatedVOnNumberModifiers));
    }
    if !registry.has_rule("vue/no-deprecated-v-bind-sync") {
        registry.register(Box::new(NoDeprecatedVBindSync));
    }
    if !registry.has_rule("vue/no-deprecated-html-element-is") {
        registry.register(Box::new(NoDeprecatedHtmlElementIs));
    }
    if !registry.has_rule("vue/no-deprecated-inline-template") {
        registry.register(Box::new(NoDeprecatedInlineTemplate));
    }
    if !registry.has_rule("vue/no-deprecated-functional-template") {
        registry.register(Box::new(NoDeprecatedFunctionalTemplate));
    }
    if !registry.has_rule("vue/no-deprecated-slot-scope-attribute") {
        registry.register(Box::new(NoDeprecatedSlotScopeAttribute));
    }
    if !registry.has_rule("vue/no-deprecated-scope-attribute") {
        registry.register(Box::new(NoDeprecatedScopeAttribute));
    }
    if !registry.has_rule("vue/no-deprecated-router-link-tag-prop") {
        registry.register(Box::new(NoDeprecatedRouterLinkTagProp));
    }
    if !registry.has_rule("vue/no-deprecated-filter") {
        registry.register(Box::new(NoDeprecatedFilter));
    }
    if !registry.has_rule("vue/no-bare-strings-in-template") {
        registry.register(Box::new(NoBareStringsInTemplate));
    }
    if !registry.has_rule("vue/no-static-inline-styles") {
        registry.register(Box::new(NoStaticInlineStyles));
    }
    if !registry.has_rule("vue/no-non-component-keep-alive-child") {
        registry.register(Box::new(NoNonComponentKeepAliveChild));
    }
}
