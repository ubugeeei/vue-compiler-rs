use vize_patina::LintPreset;
use wasm_bindgen::JsValue;

enum WasmPresetSelection {
    Builtin(LintPreset),
    Ecosystem,
}

fn parse_lint_preset(options: &JsValue) -> WasmPresetSelection {
    js_sys::Reflect::get(options, &JsValue::from_str("preset"))
        .ok()
        .and_then(|v| v.as_string())
        .as_deref()
        .and_then(|value| match value {
            "general-recommended" | "GeneralRecommended" | "generalRecommended" => {
                Some(WasmPresetSelection::Builtin(LintPreset::HappyPath))
            }
            "essential" | "Essential" => Some(WasmPresetSelection::Builtin(LintPreset::Essential)),
            "incremental" | "Incremental" => {
                Some(WasmPresetSelection::Builtin(LintPreset::Incremental))
            }
            "opinionated" | "Opinionated" | "Opnionated" | "opnionated" => {
                Some(WasmPresetSelection::Builtin(LintPreset::Opinionated))
            }
            "ecosystem" | "Ecosystem" | "eco" | "Eco" => Some(WasmPresetSelection::Ecosystem),
            "nuxt" | "Nuxt" => Some(WasmPresetSelection::Builtin(LintPreset::Nuxt)),
            _ => LintPreset::parse(value).map(WasmPresetSelection::Builtin),
        })
        .unwrap_or(WasmPresetSelection::Builtin(LintPreset::default()))
}

fn parse_enabled_rules(options: &JsValue) -> Option<Vec<vize_s0::CompactString>> {
    js_sys::Reflect::get(options, &JsValue::from_str("enabledRules"))
        .ok()
        .and_then(|v| {
            if v.is_undefined() || v.is_null() {
                return None;
            }
            js_sys::Array::from(&v)
                .iter()
                .map(|item| item.as_string().map(Into::into))
                .collect::<Option<Vec<vize_s0::CompactString>>>()
        })
}

fn parse_component_name_in_template_casing(
    options: &JsValue,
) -> Option<vize_patina::rules::ComponentCasing> {
    match js_sys::Reflect::get(options, &JsValue::from_str("componentNameInTemplateCasing"))
        .ok()
        .and_then(|v| v.as_string())
        .as_deref()
    {
        Some("PascalCase") => Some(vize_patina::rules::ComponentCasing::PascalCase),
        Some("kebab-case") => Some(vize_patina::rules::ComponentCasing::KebabCase),
        _ => None,
    }
}

fn parse_custom_event_name_casing(
    options: &JsValue,
) -> Option<vize_patina::rules::script::EventNameCasing> {
    match js_sys::Reflect::get(options, &JsValue::from_str("customEventNameCasing"))
        .ok()
        .and_then(|v| v.as_string())
        .as_deref()
    {
        Some("camelCase") => Some(vize_patina::rules::script::EventNameCasing::CamelCase),
        Some("kebab-case") => Some(vize_patina::rules::script::EventNameCasing::KebabCase),
        _ => None,
    }
}

fn parse_no_mutating_props(
    options: &JsValue,
) -> Option<vize_patina::rules::NoMutatingPropsOptions> {
    let value = js_sys::Reflect::get(options, &JsValue::from_str("noMutatingProps")).ok()?;
    if value.is_undefined() || value.is_null() {
        return None;
    }

    Some(vize_patina::rules::NoMutatingPropsOptions {
        shallow_only: js_sys::Reflect::get(&value, &JsValue::from_str("shallowOnly"))
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    })
}

fn parse_sfc_element_order(
    options: &JsValue,
) -> Option<vize_patina::rules::SfcElementOrderOptions> {
    let value = js_sys::Reflect::get(options, &JsValue::from_str("sfcElementOrder")).ok()?;
    if value.is_undefined() || value.is_null() {
        return None;
    }

    let order = js_sys::Reflect::get(&value, &JsValue::from_str("order")).ok()?;
    if order.is_undefined() || order.is_null() || !js_sys::Array::is_array(&order) {
        return Some(vize_patina::rules::SfcElementOrderOptions::default());
    }

    Some(vize_patina::rules::SfcElementOrderOptions {
        order: js_sys::Array::from(&order)
            .iter()
            .filter_map(sfc_element_order_group)
            .collect(),
    })
}

fn sfc_element_order_group(value: JsValue) -> Option<vize_patina::rules::SfcElementOrderGroup> {
    if let Some(selector) = value.as_string() {
        return Some(vize_patina::rules::SfcElementOrderGroup::new(vec![
            selector.into(),
        ]));
    }

    if !js_sys::Array::is_array(&value) {
        return None;
    }
    let selectors = js_sys::Array::from(&value)
        .iter()
        .filter_map(|value| value.as_string().map(Into::into))
        .collect::<Vec<_>>();
    (!selectors.is_empty()).then(|| vize_patina::rules::SfcElementOrderGroup::new(selectors))
}

fn parse_html_self_closing(
    options: &JsValue,
) -> Option<vize_patina::rules::HtmlSelfClosingOptions> {
    let value = js_sys::Reflect::get(options, &JsValue::from_str("htmlSelfClosing")).ok()?;
    if value.is_undefined() || value.is_null() {
        return None;
    }

    let mut resolved = vize_patina::rules::HtmlSelfClosingOptions::default();
    if let Ok(html) = js_sys::Reflect::get(&value, &JsValue::from_str("html"))
        && !html.is_undefined()
        && !html.is_null()
    {
        if let Some(style) = html_self_closing_style(&html, "void") {
            resolved.html.void = style;
        }
        if let Some(style) = html_self_closing_style(&html, "normal") {
            resolved.html.normal = style;
        }
        if let Some(style) = html_self_closing_style(&html, "component") {
            resolved.html.component = style;
        }
    }
    if let Some(style) = html_self_closing_style(&value, "svg") {
        resolved.svg = style;
    }
    if let Some(style) = html_self_closing_style(&value, "math") {
        resolved.math = style;
    }
    Some(resolved)
}

fn html_self_closing_style(
    value: &JsValue,
    key: &str,
) -> Option<vize_patina::rules::HtmlSelfClosingStyle> {
    match js_sys::Reflect::get(value, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_string())
        .as_deref()
    {
        Some("always") => Some(vize_patina::rules::HtmlSelfClosingStyle::Always),
        Some("never") => Some(vize_patina::rules::HtmlSelfClosingStyle::Never),
        Some("any") => Some(vize_patina::rules::HtmlSelfClosingStyle::Any),
        _ => None,
    }
}

fn parse_v_on_event_hyphenation(
    options: &JsValue,
) -> Option<vize_patina::rules::VOnEventHyphenationStyle> {
    match js_sys::Reflect::get(options, &JsValue::from_str("vOnEventHyphenation"))
        .ok()
        .and_then(|v| v.as_string())
        .as_deref()
    {
        Some("always") => Some(vize_patina::rules::VOnEventHyphenationStyle::Always),
        Some("never") => Some(vize_patina::rules::VOnEventHyphenationStyle::Never),
        _ => None,
    }
}

fn parse_attribute_hyphenation(options: &JsValue) -> Option<vize_patina::rules::HyphenationStyle> {
    match js_sys::Reflect::get(options, &JsValue::from_str("attributeHyphenation"))
        .ok()
        .and_then(|v| v.as_string())
        .as_deref()
    {
        Some("always") => Some(vize_patina::rules::HyphenationStyle::Always),
        Some("never") => Some(vize_patina::rules::HyphenationStyle::Never),
        _ => None,
    }
}

pub(super) fn create_linter(locale: vize_patina::Locale, options: &JsValue) -> vize_patina::Linter {
    let enabled_rules = parse_enabled_rules(options);
    let preset = if enabled_rules.is_some() {
        WasmPresetSelection::Builtin(LintPreset::Opinionated)
    } else {
        parse_lint_preset(options)
    };

    let mut linter = match preset {
        WasmPresetSelection::Builtin(preset) => vize_patina::Linter::with_preset(preset),
        WasmPresetSelection::Ecosystem => vize_patina::Linter::with_ecosystem(),
    }
    .with_locale(locale)
    .with_enabled_rules(enabled_rules);
    if let Some(casing) = parse_component_name_in_template_casing(options) {
        linter = linter.with_component_name_in_template_casing(casing);
    }
    if let Some(casing) = parse_custom_event_name_casing(options) {
        linter = linter.with_custom_event_name_casing(casing);
    }
    if let Some(options) = parse_no_mutating_props(options) {
        linter = linter.with_no_mutating_props_options(options);
    }
    if let Some(options) = parse_sfc_element_order(options) {
        linter = linter.with_sfc_element_order_options(options);
    }
    if let Some(options) = parse_html_self_closing(options) {
        linter = linter.with_html_self_closing_options(options);
    }
    if let Some(style) = parse_v_on_event_hyphenation(options) {
        linter = linter.with_v_on_event_hyphenation(style);
    }
    if let Some(style) = parse_attribute_hyphenation(options) {
        linter = linter.with_attribute_hyphenation(style);
    }
    linter
}
