mod component_name_in_template_casing;
mod html_button_has_type;
mod html_self_closing;
mod multi_word_component_names;
mod no_array_index_key;
mod no_boolean_attr_value;
mod no_empty_component_block;
mod no_inline_style;
mod no_multiple_objects_in_class;
mod no_negated_v_if_condition;
mod no_preprocessor_lang;
mod no_root_v_if;
mod no_script_non_standard_lang;
mod no_src_attribute;
mod no_template_lang;
mod no_template_shadow;
mod no_unused_refs;
mod no_useless_mustaches;
mod no_useless_v_bind;
mod no_v_text;
mod prefer_props_shorthand;
mod prefer_true_attribute_shorthand;
mod require_component_registration;
mod scoped_event_names;
mod slot_name_casing;
mod this_in_template;
mod use_unique_element_ids;
mod use_v_on_exact;
mod v_bind_style;
mod v_on_event_hyphenation;
mod v_on_handler_style;
mod warn_custom_block;
mod warn_custom_directive;

use crate::rule::RuleRegistry;

pub(crate) use component_name_in_template_casing::ComponentNameInTemplateCasingNuxt;
pub use component_name_in_template_casing::{ComponentCasing, ComponentNameInTemplateCasing};
pub use html_button_has_type::HtmlButtonHasType;
pub(crate) use html_self_closing::HtmlSelfClosingNuxt;
pub use html_self_closing::{
    HtmlSelfClosing, HtmlSelfClosingHtmlOptions, HtmlSelfClosingOptions, HtmlSelfClosingStyle,
};
pub use multi_word_component_names::MultiWordComponentNames;
pub use no_array_index_key::NoArrayIndexKey;
pub use no_boolean_attr_value::NoBooleanAttrValue;
pub use no_empty_component_block::NoEmptyComponentBlock;
pub use no_inline_style::NoInlineStyle;
pub use no_multiple_objects_in_class::NoMultipleObjectsInClass;
pub use no_negated_v_if_condition::NoNegatedVIfCondition;
pub use no_preprocessor_lang::NoPreprocessorLang;
pub use no_root_v_if::NoRootVIf;
pub use no_script_non_standard_lang::NoScriptNonStandardLang;
pub use no_src_attribute::NoSrcAttribute;
pub use no_template_lang::NoTemplateLang;
pub use no_template_shadow::NoTemplateShadow;
pub use no_unused_refs::NoUnusedRefs;
pub use no_useless_mustaches::NoUselessMustaches;
pub use no_useless_v_bind::NoUselessVBind;
pub use no_v_text::NoVText;
pub use prefer_props_shorthand::PreferPropsShorthand;
pub use prefer_true_attribute_shorthand::PreferTrueAttributeShorthand;
pub use require_component_registration::RequireComponentRegistration;
pub use scoped_event_names::ScopedEventNames;
pub use slot_name_casing::SlotNameCasing;
pub use this_in_template::ThisInTemplate;
pub use use_unique_element_ids::UseUniqueElementIds;
pub use use_v_on_exact::UseVOnExact;
pub use v_bind_style::{VBindStyle, VBindStyleOption};
pub use v_on_event_hyphenation::{VOnEventHyphenation, VOnEventHyphenationStyle};
pub use v_on_handler_style::VOnHandlerStyle;
pub use warn_custom_block::WarnCustomBlock;
pub use warn_custom_directive::WarnCustomDirective;

pub(crate) fn register(registry: &mut RuleRegistry) {
    register_shared(registry, PresetFlavor::Default);
    registry.register(Box::new(RequireComponentRegistration::default()));
}

pub(crate) fn register_nuxt(registry: &mut RuleRegistry) {
    register_shared(registry, PresetFlavor::Nuxt);
}

#[derive(Clone, Copy)]
enum PresetFlavor {
    Default,
    Nuxt,
}

fn register_shared(registry: &mut RuleRegistry, flavor: PresetFlavor) {
    registry.register(Box::new(MultiWordComponentNames::default()));
    registry.register(Box::new(UseVOnExact));

    registry.register(Box::new(NoTemplateShadow));
    registry.register(Box::new(VBindStyle::default()));
    registry.register(Box::new(VOnHandlerStyle));
    match flavor {
        PresetFlavor::Default => registry.register(Box::new(HtmlSelfClosing::default())),
        PresetFlavor::Nuxt => registry.register(Box::new(HtmlSelfClosingNuxt::default())),
    }
    registry.register(Box::new(HtmlButtonHasType));
    registry.register(Box::new(ScopedEventNames));
    registry.register(Box::new(PreferPropsShorthand));
    registry.register(Box::new(SlotNameCasing));

    registry.register(Box::new(UseUniqueElementIds::default()));

    match flavor {
        PresetFlavor::Default => {
            registry.register(Box::new(ComponentNameInTemplateCasing::default()))
        }
        PresetFlavor::Nuxt => {
            registry.register(Box::new(ComponentNameInTemplateCasingNuxt::default()))
        }
    }
    registry.register(Box::new(NoPreprocessorLang));
    registry.register(Box::new(NoScriptNonStandardLang));
    registry.register(Box::new(NoTemplateLang));
    registry.register(Box::new(NoSrcAttribute));
    registry.register(Box::new(NoInlineStyle));
    registry.register(Box::new(NoMultipleObjectsInClass));
    registry.register(Box::new(WarnCustomBlock));
    registry.register(Box::new(WarnCustomDirective));
    registry.register(Box::new(NoBooleanAttrValue));
    registry.register(Box::new(NoEmptyComponentBlock));
    registry.register(Box::new(NoUselessMustaches));
    registry.register(Box::new(NoRootVIf));
    registry.register(Box::new(NoUselessVBind));
    registry.register(Box::new(NoNegatedVIfCondition));
    registry.register(Box::new(NoVText));
    registry.register(Box::new(NoUnusedRefs));
    registry.register(Box::new(PreferTrueAttributeShorthand));
    registry.register(Box::new(VOnEventHyphenation::default()));
    registry.register(Box::new(ThisInTemplate));
    registry.register(Box::new(NoArrayIndexKey));
}
