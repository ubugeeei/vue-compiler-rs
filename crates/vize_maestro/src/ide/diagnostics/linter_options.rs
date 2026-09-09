use tower_lsp::lsp_types::Url;
use vize_patina::{
    Severity,
    rules::musea::{MuseaLintResult, MuseaLinter, PreferDesignTokensConfig},
};
use vize_s0::{
    String,
    config::{ConfigLintRuleOptions, LintRuleSeverity, LinterConfig},
};

const MUSEA_PREFER_DESIGN_TOKENS: &str = "musea/prefer-design-tokens";
const SCRIPT_NO_RESTRICTED_MEMBERS: &str = "script/no-restricted-members";

pub(super) struct PatinaLintOptions {
    pub(super) additional_rules: Vec<String>,
    pub(super) disabled_rules: Vec<String>,
    pub(super) restricted_globals: Vec<(String, Option<String>)>,
    pub(super) restricted_members: Vec<(String, String, Option<String>)>,
    pub(super) musea_design_tokens: Vec<(String, String, String)>,
    pub(super) category_severity_overrides: Vec<(String, Severity)>,
    pub(super) rule_severity_overrides: Vec<(String, Severity)>,
}

pub(super) struct MuseaLintOptions {
    linter: MuseaLinter,
    rule_severity_overrides: Vec<(String, Severity)>,
}

impl MuseaLintOptions {
    pub(super) fn lint(&self, source: &str) -> MuseaLintResult {
        let mut result = self.linter.lint(source);
        if self.rule_severity_overrides.is_empty() {
            return result;
        }

        for diagnostic in &mut result.diagnostics {
            if let Some((_, severity)) = self
                .rule_severity_overrides
                .iter()
                .find(|(rule, _)| rule.as_str() == diagnostic.rule_name)
            {
                diagnostic.severity = *severity;
            }
        }
        result.error_count = result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count();
        result.warning_count = result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Warning)
            .count();
        result
    }
}

pub(super) fn resolve_patina_options(
    linter_config: &LinterConfig,
    rule_options: &ConfigLintRuleOptions,
) -> PatinaLintOptions {
    let mut additional_rules = linter_config.enabled_rules();
    let disabled_rules = linter_config.disabled_rules();
    let restricted_members = rule_options.restricted_members();
    let musea_design_tokens = rule_options.musea_design_tokens();

    add_project_local_rule(
        &mut additional_rules,
        &disabled_rules,
        SCRIPT_NO_RESTRICTED_MEMBERS,
        !restricted_members.is_empty(),
    );
    add_project_local_rule(
        &mut additional_rules,
        &disabled_rules,
        MUSEA_PREFER_DESIGN_TOKENS,
        !musea_design_tokens.is_empty(),
    );

    PatinaLintOptions {
        additional_rules,
        disabled_rules,
        restricted_globals: rule_options.restricted_globals(),
        restricted_members,
        musea_design_tokens,
        category_severity_overrides: severity_overrides(
            linter_config.category_severity_overrides(),
        ),
        rule_severity_overrides: severity_overrides(linter_config.rule_severity_overrides()),
    }
}

pub(super) fn musea_linter_for_uri(
    state: &crate::server::ServerState,
    uri: &Url,
) -> Option<MuseaLintOptions> {
    let (linter_config, rule_options) = state.linter_settings_for_uri(uri);
    if !linter_config.enabled {
        return None;
    }

    let rule_severity_overrides = severity_overrides(linter_config.rule_severity_overrides());
    let tokens = rule_options.musea_design_tokens();
    if tokens.is_empty()
        || linter_config
            .disabled_rules()
            .iter()
            .any(|rule| rule.as_str() == MUSEA_PREFER_DESIGN_TOKENS)
    {
        return Some(MuseaLintOptions {
            linter: MuseaLinter::new(),
            rule_severity_overrides,
        });
    }

    let mut config = PreferDesignTokensConfig::default();
    for (value, path, tier) in tokens {
        config.add_token(&value, &path, &tier);
    }
    Some(MuseaLintOptions {
        linter: MuseaLinter::new().with_design_tokens(config),
        rule_severity_overrides,
    })
}

pub(super) fn apply_rule_options(
    mut linter: vize_patina::Linter,
    options: &vize_s0::config::ConfigLintRuleOptions,
) -> vize_patina::Linter {
    if let Some(casing) = options.component_name_in_template_casing() {
        linter = linter.with_component_name_in_template_casing(component_casing(casing));
    }
    if let Some(casing) = options.custom_event_name_casing() {
        linter = linter.with_custom_event_name_casing(event_name_casing(casing));
    }
    if let Some(options) = options.html_self_closing() {
        linter = linter.with_html_self_closing_options(html_self_closing_options(options));
    }
    linter
}

fn add_project_local_rule(
    additional_rules: &mut Vec<String>,
    disabled_rules: &[String],
    rule_name: &str,
    configured: bool,
) {
    if configured
        && !disabled_rules.iter().any(|rule| rule.as_str() == rule_name)
        && !additional_rules
            .iter()
            .any(|rule| rule.as_str() == rule_name)
    {
        additional_rules.push(rule_name.into());
    }
}

fn severity_overrides(entries: Vec<(String, LintRuleSeverity)>) -> Vec<(String, Severity)> {
    entries
        .into_iter()
        .filter_map(|(name, severity)| match severity {
            LintRuleSeverity::Off => None,
            LintRuleSeverity::Warn => Some((name, Severity::Warning)),
            LintRuleSeverity::Error => Some((name, Severity::Error)),
        })
        .collect()
}

fn component_casing(
    casing: vize_s0::config::TemplateComponentNameCasing,
) -> vize_patina::rules::ComponentCasing {
    match casing {
        vize_s0::config::TemplateComponentNameCasing::PascalCase => {
            vize_patina::rules::ComponentCasing::PascalCase
        }
        vize_s0::config::TemplateComponentNameCasing::KebabCase => {
            vize_patina::rules::ComponentCasing::KebabCase
        }
    }
}

fn event_name_casing(
    casing: vize_s0::config::CustomEventNameCasing,
) -> vize_patina::rules::script::EventNameCasing {
    match casing {
        vize_s0::config::CustomEventNameCasing::CamelCase => {
            vize_patina::rules::script::EventNameCasing::CamelCase
        }
        vize_s0::config::CustomEventNameCasing::KebabCase => {
            vize_patina::rules::script::EventNameCasing::KebabCase
        }
    }
}

fn html_self_closing_options(
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
