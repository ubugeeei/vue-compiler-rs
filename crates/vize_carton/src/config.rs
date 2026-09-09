//! Shared Vize configuration loading.

mod loader;
mod model;
mod normalize;

pub use loader::{
    LoadedConfig, LoadedConfigEntryFiles, LoadedConfigEntryIgnores, LoadedConfigWithFeatures,
    load_compiler_custom_elements, load_compiler_host_compiler, load_compiler_jsx_compat,
    load_compiler_jsx_mode, load_compiler_template_syntax, load_compiler_vapor,
    load_compiler_vue_version, load_config,
    load_config_and_linter_plan_with_config_rule_options_and_lint_features_and_source,
    load_config_and_linter_plan_with_lint_features_and_source,
    load_config_and_linter_plan_with_rule_options_and_lint_features_and_source,
    load_config_and_linter_with_features_and_source,
    load_config_and_linter_with_lint_features_and_source, load_config_and_linter_with_source,
    load_config_entry_files_with_source, load_config_entry_ignores_with_source,
    load_config_lint_rule_options, load_config_with_features_and_source, load_config_with_source,
    load_language_server_unstable_flags, load_linter_config, load_linter_rule_options,
    validate_explicit_config_path,
};
pub use model::{
    ArrowParens, AttributeSortOrder, ComponentNameInTemplateCasingOptions, ConfigEntryFiles,
    ConfigEntryIgnore, ConfigFeatureFlags, ConfigLintRuleOptions, CustomEventNameCasing,
    CustomEventNameCasingOptions, EndOfLine, FormatterConfig, GlobalTypeDeclaration,
    GlobalTypesConfig, HtmlSelfClosingHtmlOptions, HtmlSelfClosingOptions, HtmlSelfClosingStyle,
    HyphenationStyle, JsxCompat, JsxMode, LanguageServerConfig, LanguageServerUnstableFlags,
    LintRuleOptions, LintRuleSeverity, LinterConfig, LinterConfigEntry, LinterConfigPlan,
    LinterConfigPlanWithConfigRuleOptions, LinterConfigPlanWithRuleOptions, LinterFeatureFlags,
    LspConfig, MuseaDesignToken, MuseaPreferDesignTokensOptions, NoMutatingPropsOptions,
    NoRestrictedGlobalsOptions, NoRestrictedMembersOptions, ParseVueVersionError, QuoteProps,
    ResolvedLinterConfig, ResolvedLinterConfigWithConfigRuleOptions, RestrictedGlobal,
    RestrictedMember, SfcElementOrderGroup, SfcElementOrderOptions, TemplateComponentNameCasing,
    TrailingComma, TypeCheckerConfig, VizeConfig, VueVersion,
};
pub use normalize::normalize_public_config_value;

pub use crate::dialect::VueDialect;
