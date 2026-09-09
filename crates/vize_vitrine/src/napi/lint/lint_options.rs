use napi_derive::napi;
use std::path::PathBuf;

/// Lint options for NAPI
#[napi(object)]
#[derive(Default)]
pub struct LintOptionsNapi {
    /// Output format: "text", "ansi", "plain", "json", "stylish", "markdown", "html", or "agent"
    pub format: Option<String>,
    /// Maximum number of warnings before failing
    pub max_warnings: Option<u32>,
    /// Quiet mode - only show summary
    pub quiet: Option<bool>,
    /// Automatically fix problems when diagnostics provide safe text edits
    pub fix: Option<bool>,
    /// Help display level: "full", "short", "none"
    pub help_level: Option<String>,
    /// Lint preset: "general-recommended", "essential", "incremental", "ecosystem", "opinionated", or "nuxt"
    pub preset: Option<String>,
    /// Enable native type-aware lint rules
    pub type_aware: Option<bool>,
    /// Path to the Corsa executable used by type-aware lint rules
    pub corsa_path: Option<String>,
}

/// Lint result for NAPI
#[napi(object)]
pub struct LintResultNapi {
    /// Formatted output string
    pub output: String,
    /// Total number of errors
    pub error_count: u32,
    /// Total number of warnings
    pub warning_count: u32,
    /// Number of files linted
    pub file_count: u32,
    /// Time in milliseconds
    pub time_ms: f64,
}

/// Single-file Patina lint options for NAPI
#[napi(object)]
#[derive(Default)]
pub struct PatinaLintOptionsNapi {
    /// Filename used for diagnostics
    pub filename: Option<String>,
    /// Locale code: "en", "ja", or "zh"
    pub locale: Option<String>,
    /// Help display level: "full", "short", or "none"
    pub help_level: Option<String>,
    /// Lint preset: "general-recommended", "essential", "incremental", "ecosystem", "opinionated", or "nuxt"
    pub preset: Option<String>,
    /// Optional list of Patina rule names to enable
    pub enabled_rules: Option<Vec<String>>,
    /// Enable native type-aware lint rules
    pub type_aware: Option<bool>,
    /// Path to the Corsa executable used by type-aware lint rules
    pub corsa_path: Option<String>,
    /// Casing for `vue/component-name-in-template-casing`: "PascalCase" or "kebab-case"
    pub component_name_in_template_casing: Option<String>,
    /// Casing for `script/custom-event-name-casing`: "camelCase" or "kebab-case"
    pub custom_event_name_casing: Option<String>,
    /// Options for `vue/html-self-closing`
    pub html_self_closing: Option<HtmlSelfClosingOptionsNapi>,
}

/// HTML self-closing options for NAPI
#[napi(object)]
#[derive(Default)]
pub struct HtmlSelfClosingOptionsNapi {
    pub html: Option<HtmlSelfClosingHtmlOptionsNapi>,
    pub svg: Option<String>,
    pub math: Option<String>,
}

/// HTML-family self-closing options for NAPI
#[napi(object)]
#[derive(Default)]
pub struct HtmlSelfClosingHtmlOptionsNapi {
    pub r#void: Option<String>,
    pub normal: Option<String>,
    pub component: Option<String>,
}

pub(super) enum PatinaPresetSelection {
    Builtin(vize_patina::LintPreset),
    Ecosystem,
}

pub(super) fn patina_locale_from_option(locale: Option<&str>) -> vize_patina::Locale {
    locale
        .and_then(vize_patina::Locale::parse)
        .unwrap_or_default()
}

pub(super) fn patina_help_level_from_option(help_level: Option<&str>) -> vize_patina::HelpLevel {
    match help_level {
        Some("none") => vize_patina::HelpLevel::None,
        Some("short") => vize_patina::HelpLevel::Short,
        _ => vize_patina::HelpLevel::Full,
    }
}

pub(super) fn patina_preset_from_option(preset: Option<&str>) -> PatinaPresetSelection {
    match preset {
        Some("general-recommended" | "GeneralRecommended" | "generalRecommended")
        | Some("happy-path" | "happy_path" | "happy" | "default" | "recommended") => {
            PatinaPresetSelection::Builtin(vize_patina::LintPreset::HappyPath)
        }
        Some("essential" | "Essential") => {
            PatinaPresetSelection::Builtin(vize_patina::LintPreset::Essential)
        }
        Some("incremental" | "Incremental") => {
            PatinaPresetSelection::Builtin(vize_patina::LintPreset::Incremental)
        }
        Some("ecosystem" | "Ecosystem" | "eco" | "Eco") => PatinaPresetSelection::Ecosystem,
        Some("opinionated" | "Opinionated" | "Opnionated" | "opnionated" | "strict" | "all") => {
            PatinaPresetSelection::Builtin(vize_patina::LintPreset::Opinionated)
        }
        Some("nuxt" | "Nuxt") => PatinaPresetSelection::Builtin(vize_patina::LintPreset::Nuxt),
        _ => PatinaPresetSelection::Builtin(vize_patina::LintPreset::default()),
    }
}

pub(super) fn create_patina_linter(preset: PatinaPresetSelection) -> vize_patina::Linter {
    match preset {
        PatinaPresetSelection::Builtin(preset) => vize_patina::Linter::with_preset(preset),
        PatinaPresetSelection::Ecosystem => vize_patina::Linter::with_ecosystem(),
    }
}

pub(super) fn configure_type_aware_lint(
    linter: vize_patina::Linter,
    type_aware: Option<bool>,
    corsa_path: Option<String>,
) -> vize_patina::Linter {
    linter
        .with_type_aware_lint(type_aware.unwrap_or(false))
        .with_corsa_path(corsa_path.map(PathBuf::from))
}

pub(super) fn configure_patina_rule_options(
    mut linter: vize_patina::Linter,
    component_name_in_template_casing: Option<&str>,
    custom_event_name_casing: Option<&str>,
    html_self_closing: Option<HtmlSelfClosingOptionsNapi>,
) -> vize_patina::Linter {
    if let Some(casing) = component_name_in_template_casing.and_then(component_casing_from_option) {
        linter = linter.with_component_name_in_template_casing(casing);
    }
    if let Some(casing) = custom_event_name_casing.and_then(event_name_casing_from_option) {
        linter = linter.with_custom_event_name_casing(casing);
    }
    if let Some(options) = html_self_closing.map(html_self_closing_from_option) {
        linter = linter.with_html_self_closing_options(options);
    }
    linter
}

pub(super) fn component_casing_from_option(
    value: &str,
) -> Option<vize_patina::rules::ComponentCasing> {
    match value {
        "PascalCase" => Some(vize_patina::rules::ComponentCasing::PascalCase),
        "kebab-case" => Some(vize_patina::rules::ComponentCasing::KebabCase),
        _ => None,
    }
}

pub(super) fn event_name_casing_from_option(
    value: &str,
) -> Option<vize_patina::rules::script::EventNameCasing> {
    match value {
        "camelCase" => Some(vize_patina::rules::script::EventNameCasing::CamelCase),
        "kebab-case" => Some(vize_patina::rules::script::EventNameCasing::KebabCase),
        _ => None,
    }
}

fn html_self_closing_from_option(
    options: HtmlSelfClosingOptionsNapi,
) -> vize_patina::rules::HtmlSelfClosingOptions {
    let mut resolved = vize_patina::rules::HtmlSelfClosingOptions::default();
    if let Some(html) = options.html {
        if let Some(style) = html.r#void.as_deref().and_then(html_self_closing_style) {
            resolved.html.void = style;
        }
        if let Some(style) = html.normal.as_deref().and_then(html_self_closing_style) {
            resolved.html.normal = style;
        }
        if let Some(style) = html.component.as_deref().and_then(html_self_closing_style) {
            resolved.html.component = style;
        }
    }
    if let Some(style) = options.svg.as_deref().and_then(html_self_closing_style) {
        resolved.svg = style;
    }
    if let Some(style) = options.math.as_deref().and_then(html_self_closing_style) {
        resolved.math = style;
    }
    resolved
}

fn html_self_closing_style(value: &str) -> Option<vize_patina::rules::HtmlSelfClosingStyle> {
    match value {
        "always" => Some(vize_patina::rules::HtmlSelfClosingStyle::Always),
        "never" => Some(vize_patina::rules::HtmlSelfClosingStyle::Never),
        "any" => Some(vize_patina::rules::HtmlSelfClosingStyle::Any),
        _ => None,
    }
}
