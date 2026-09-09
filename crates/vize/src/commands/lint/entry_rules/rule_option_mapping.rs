use vize_s0::config::{
    CustomEventNameCasing as ConfigEventNameCasing, TemplateComponentNameCasing,
};

pub(super) fn component_casing(
    casing: TemplateComponentNameCasing,
) -> vize_patina::rules::ComponentCasing {
    match casing {
        TemplateComponentNameCasing::PascalCase => vize_patina::rules::ComponentCasing::PascalCase,
        TemplateComponentNameCasing::KebabCase => vize_patina::rules::ComponentCasing::KebabCase,
    }
}

pub(super) fn event_name_casing(
    casing: ConfigEventNameCasing,
) -> vize_patina::rules::script::EventNameCasing {
    match casing {
        ConfigEventNameCasing::CamelCase => vize_patina::rules::script::EventNameCasing::CamelCase,
        ConfigEventNameCasing::KebabCase => vize_patina::rules::script::EventNameCasing::KebabCase,
    }
}

pub(super) fn no_mutating_props_options(
    options: vize_s0::config::NoMutatingPropsOptions,
) -> vize_patina::rules::NoMutatingPropsOptions {
    vize_patina::rules::NoMutatingPropsOptions {
        shallow_only: options.shallow_only,
    }
}

pub(super) fn sfc_element_order_options(
    options: vize_s0::config::SfcElementOrderOptions,
) -> vize_patina::rules::SfcElementOrderOptions {
    vize_patina::rules::SfcElementOrderOptions {
        order: options
            .order
            .into_iter()
            .map(|group| vize_patina::rules::SfcElementOrderGroup::new(group.selectors()))
            .collect(),
    }
}

pub(super) fn html_self_closing_options(
    options: vize_s0::config::HtmlSelfClosingOptions,
) -> vize_patina::rules::HtmlSelfClosingOptions {
    vize_patina::rules::HtmlSelfClosingOptions {
        html: vize_patina::rules::HtmlSelfClosingHtmlOptions {
            void: html_self_closing_style(options.html.void_elements),
            normal: html_self_closing_style(options.html.normal),
            component: html_self_closing_style(options.html.component),
        },
        svg: html_self_closing_style(options.svg),
        math: html_self_closing_style(options.math),
    }
}

fn html_self_closing_style(
    style: vize_s0::config::HtmlSelfClosingStyle,
) -> vize_patina::rules::HtmlSelfClosingStyle {
    match style {
        vize_s0::config::HtmlSelfClosingStyle::Always => {
            vize_patina::rules::HtmlSelfClosingStyle::Always
        }
        vize_s0::config::HtmlSelfClosingStyle::Never => {
            vize_patina::rules::HtmlSelfClosingStyle::Never
        }
        vize_s0::config::HtmlSelfClosingStyle::Any => vize_patina::rules::HtmlSelfClosingStyle::Any,
    }
}

pub(super) fn v_on_event_hyphenation_style(
    style: vize_s0::config::HyphenationStyle,
) -> vize_patina::rules::VOnEventHyphenationStyle {
    match style {
        vize_s0::config::HyphenationStyle::Always => {
            vize_patina::rules::VOnEventHyphenationStyle::Always
        }
        vize_s0::config::HyphenationStyle::Never => {
            vize_patina::rules::VOnEventHyphenationStyle::Never
        }
    }
}

pub(super) fn attribute_hyphenation_style(
    style: vize_s0::config::HyphenationStyle,
) -> vize_patina::rules::HyphenationStyle {
    match style {
        vize_s0::config::HyphenationStyle::Always => vize_patina::rules::HyphenationStyle::Always,
        vize_s0::config::HyphenationStyle::Never => vize_patina::rules::HyphenationStyle::Never,
    }
}
