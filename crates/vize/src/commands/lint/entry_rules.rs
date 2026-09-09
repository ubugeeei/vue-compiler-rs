//! Batch resolution of declaration-ordered `entries[].linter.rules`.

use std::path::{Path, PathBuf};
use vize_patina::{HelpLevel, LintPreset, Linter, Severity};
use vize_s0::{
    FxHashMap, String,
    config::{
        ConfigLintRuleOptions, LintRuleSeverity, LinterConfigPlanWithConfigRuleOptions,
        LinterFeatureFlags,
    },
};

mod rule_option_mapping;

use self::rule_option_mapping::{
    attribute_hyphenation_style, component_casing, event_name_casing, html_self_closing_options,
    no_mutating_props_options, sfc_element_order_options, v_on_event_hyphenation_style,
};
use super::LintArgs;
#[cfg(test)]
use crate::lint_plan::matcher::GlobSequence;
use crate::lint_plan::matcher::{LintPlanScope, absolute_path};

const PINIA_PREFER_STORE_TO_REFS: &str = "ecosystem/pinia-prefer-store-to-refs";
const MUSEA_PREFER_DESIGN_TOKENS: &str = "musea/prefer-design-tokens";
const SCRIPT_NO_RESTRICTED_MEMBERS: &str = "script/no-restricted-members";

pub(super) struct ResolvedLinterRuleGroups {
    pub(super) configs: Vec<crate::config::LinterConfig>,
    rule_options: Vec<ConfigLintRuleOptions>,
    pub(super) file_config_indices: Vec<usize>,
    pinia_available: Vec<bool>,
}

impl ResolvedLinterRuleGroups {
    pub(super) fn build_linters(
        &self,
        preset: LintPreset,
        help_level: HelpLevel,
        args: &LintArgs,
        features: LinterFeatureFlags,
        configured_corsa_path: Option<PathBuf>,
    ) -> Vec<Linter> {
        self.configs
            .iter()
            .zip(&self.pinia_available)
            .zip(&self.rule_options)
            .map(|((config, pinia_available), rule_options)| {
                let type_aware =
                    args.type_aware || args.strict_reactivity || config.type_aware_lint_enabled();
                let mut additional_rules = config.enabled_rules();
                let disabled_rules = resolved_disabled_rules(config, *pinia_available);
                let restricted_globals = rule_options.restricted_globals();
                let restricted_members = rule_options.restricted_members();
                let musea_design_tokens = rule_options.musea_design_tokens();
                if !musea_design_tokens.is_empty()
                    && !disabled_rules
                        .iter()
                        .any(|rule| rule.as_str() == MUSEA_PREFER_DESIGN_TOKENS)
                    && !additional_rules
                        .iter()
                        .any(|rule| rule.as_str() == MUSEA_PREFER_DESIGN_TOKENS)
                {
                    additional_rules.push(MUSEA_PREFER_DESIGN_TOKENS.into());
                }
                if !restricted_members.is_empty()
                    && !disabled_rules
                        .iter()
                        .any(|rule| rule.as_str() == SCRIPT_NO_RESTRICTED_MEMBERS)
                    && !additional_rules
                        .iter()
                        .any(|rule| rule.as_str() == SCRIPT_NO_RESTRICTED_MEMBERS)
                {
                    additional_rules.push(SCRIPT_NO_RESTRICTED_MEMBERS.into());
                }
                let mut linter = Linter::with_preset(preset)
                    .with_additional_rules(additional_rules)
                    .with_disabled_rules(disabled_rules)
                    .with_disabled_categories(config.disabled_categories())
                    .with_category_severity_overrides(severity_overrides(
                        config.category_severity_overrides(),
                    ))
                    .with_rule_severity_overrides(severity_overrides(
                        config.rule_severity_overrides(),
                    ))
                    .with_help_level(help_level)
                    .with_type_aware_lint(type_aware)
                    .with_vue_version(features.vue_version)
                    .with_vapor_mode(features.vapor)
                    .with_restricted_globals(restricted_globals)
                    .with_restricted_members(restricted_members)
                    .with_musea_design_tokens(musea_design_tokens);
                if let Some(casing) = rule_options.component_name_in_template_casing() {
                    linter =
                        linter.with_component_name_in_template_casing(component_casing(casing));
                }
                if let Some(casing) = rule_options.custom_event_name_casing() {
                    linter = linter.with_custom_event_name_casing(event_name_casing(casing));
                }
                if let Some(options) = rule_options.no_mutating_props() {
                    linter =
                        linter.with_no_mutating_props_options(no_mutating_props_options(options));
                }
                if let Some(options) = rule_options.sfc_element_order() {
                    linter =
                        linter.with_sfc_element_order_options(sfc_element_order_options(options));
                }
                if let Some(options) = rule_options.html_self_closing() {
                    linter =
                        linter.with_html_self_closing_options(html_self_closing_options(options));
                }
                if let Some(style) = rule_options.v_on_event_hyphenation() {
                    linter =
                        linter.with_v_on_event_hyphenation(v_on_event_hyphenation_style(style));
                }
                if let Some(style) = rule_options.attribute_hyphenation() {
                    linter = linter.with_attribute_hyphenation(attribute_hyphenation_style(style));
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    linter = linter.with_corsa_path(configured_corsa_path.clone());
                }
                #[cfg(not(target_arch = "wasm32"))]
                if args.strict_reactivity {
                    linter = linter.with_rule(Box::new(
                        vize_patina::rules::type_aware::NoReactivityLoss::new(),
                    ));
                }
                linter
            })
            .collect()
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

pub(super) struct LinterRuleResolver {
    plan: LinterConfigPlanWithConfigRuleOptions,
    scopes: Vec<LintPlanScope>,
}

impl LinterRuleResolver {
    pub(super) fn new(
        plan: impl Into<LinterConfigPlanWithConfigRuleOptions>,
        config_dir: &Path,
        cwd: &Path,
    ) -> Self {
        let plan = plan.into();
        let config_dir = absolute_path(config_dir, cwd);
        let scopes = plan
            .plan
            .entries
            .iter()
            .map(|entry| {
                LintPlanScope::new(
                    entry.base_path.as_deref(),
                    entry.files.as_deref(),
                    &entry.ignores,
                    &config_dir,
                    cwd,
                )
            })
            .collect();
        Self { plan, scopes }
    }

    /// Resolve each distinct matching-entry signature exactly once for the batch.
    pub(super) fn resolve_files(&self, files: &[PathBuf], cwd: &Path) -> ResolvedLinterRuleGroups {
        let mut signatures = FxHashMap::<(Vec<usize>, bool), usize>::default();
        let mut dependency_cache = FxHashMap::default();
        let mut configs = Vec::new();
        let mut rule_options = Vec::new();
        let mut pinia_available = Vec::new();
        let mut file_config_indices = Vec::with_capacity(files.len());
        for file in files {
            let signature = (
                self.matching_entries(file, cwd),
                package_is_resolvable_from(file, cwd, "pinia", &mut dependency_cache),
            );
            let index = match signatures.get(&signature) {
                Some(index) => *index,
                None => {
                    let index = configs.len();
                    let resolved = self.plan.resolve_matching_entries(&signature.0);
                    configs.push(resolved.config);
                    rule_options.push(resolved.rule_options);
                    pinia_available.push(signature.1);
                    signatures.insert(signature, index);
                    index
                }
            };
            file_config_indices.push(index);
        }
        ResolvedLinterRuleGroups {
            configs,
            rule_options,
            file_config_indices,
            pinia_available,
        }
    }

    fn matching_entries(&self, file: &Path, cwd: &Path) -> Vec<usize> {
        let file = absolute_path(file, cwd);
        self.scopes
            .iter()
            .enumerate()
            .filter_map(|(index, scope)| scope.matches(&file).then_some(index))
            .collect()
    }
}

fn resolved_disabled_rules(
    config: &crate::config::LinterConfig,
    pinia_available: bool,
) -> Vec<String> {
    let mut rules = config.disabled_rules();
    if !pinia_available {
        rules.push(PINIA_PREFER_STORE_TO_REFS.into());
    }
    rules
}

fn package_is_resolvable_from(
    file: &Path,
    cwd: &Path,
    package_name: &str,
    cache: &mut FxHashMap<PathBuf, bool>,
) -> bool {
    let absolute = absolute_path(file, cwd);
    let Some(directory) = absolute.parent() else {
        return false;
    };
    let mut current = directory.to_path_buf();
    let mut visited = Vec::new();
    let available = loop {
        if let Some(available) = cache.get(&current) {
            break *available;
        }
        visited.push(current.clone());
        if current
            .join("node_modules")
            .join(package_name)
            .join("package.json")
            .is_file()
        {
            break true;
        }
        let Some(parent) = current.parent() else {
            break false;
        };
        if parent == current {
            break false;
        }
        current = parent.to_path_buf();
    };
    for directory in visited {
        cache.insert(directory, available);
    }
    available
}

#[cfg(test)]
#[path = "entry_rules_tests.rs"]
mod tests;
