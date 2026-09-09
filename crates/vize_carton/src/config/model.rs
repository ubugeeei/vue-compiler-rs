//! Shared config model.

mod compatibility;
mod compiler;
mod entries;
mod experimentals;
mod formatter;
mod global_types;
mod language_server;
mod linter;
mod linter_rule_options;
mod type_checker;
mod vue;

use self::{compiler::RawCompilerConfig, experimentals::RawExperimentalsConfig, vue::RawVueConfig};
use serde::{Deserialize, Serialize};

use crate::String;
use crate::dialect::VueDialect;
pub use compiler::{JsxCompat, JsxMode};
pub(crate) use entries::RawConfigEntry;
pub use entries::{
    ConfigEntryFiles, ConfigEntryIgnore, LinterConfigEntry, LinterConfigPlan,
    LinterConfigPlanWithConfigRuleOptions, LinterConfigPlanWithRuleOptions, ResolvedLinterConfig,
    ResolvedLinterConfigWithConfigRuleOptions,
};
pub use formatter::{
    ArrowParens, AttributeSortOrder, EndOfLine, FormatterConfig, QuoteProps, TrailingComma,
};
pub use global_types::{GlobalTypeDeclaration, GlobalTypesConfig, RawGlobalTypesConfig};
pub use language_server::{LanguageServerConfig, LanguageServerUnstableFlags, LspConfig};
#[allow(unused_imports)]
pub(crate) use linter::RawLinterConfig;
pub use linter::{LintRuleSeverity, LinterConfig};
#[allow(unused_imports)]
pub use linter_rule_options::{
    ComponentNameInTemplateCasingOptions, ConfigLintRuleOptions, CustomEventNameCasing,
    CustomEventNameCasingOptions, HtmlSelfClosingHtmlOptions, HtmlSelfClosingOptions,
    HtmlSelfClosingStyle, HyphenationStyle, LintRuleOptions, MuseaDesignToken,
    MuseaPreferDesignTokensOptions, NoMutatingPropsOptions, NoRestrictedGlobalsOptions,
    NoRestrictedMembersOptions, RestrictedGlobal, RestrictedMember, SfcElementOrderGroup,
    SfcElementOrderOptions, TemplateComponentNameCasing,
};
pub use type_checker::TypeCheckerConfig;
pub use vue::{ParseVueVersionError, VueVersion};

/// Effective shared configuration.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(default)]
pub struct VizeConfig {
    /// JSON Schema reference for legacy JSON editor autocompletion.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Vue dialect profile for standalone HTML documents (`"vue"` or
    /// `"petite-vue"`). When absent, the dialect is detected structurally per
    /// document (see [`crate::dialect::standalone_html_dialect`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialect: Option<VueDialect>,
    /// Formatter settings shared by CLI and IDE formatting.
    #[serde(skip_serializing_if = "FormatterConfig::is_default")]
    pub formatter: FormatterConfig,
    /// Type checker settings shared by CLI and IDE diagnostics.
    #[serde(
        rename = "typeChecker",
        skip_serializing_if = "TypeCheckerConfig::is_default"
    )]
    pub type_checker: TypeCheckerConfig,
    /// IDE language server feature flags.
    #[serde(
        rename = "languageServer",
        skip_serializing_if = "LanguageServerConfig::is_default"
    )]
    pub language_server: LanguageServerConfig,
    /// Template global declarations.
    #[serde(
        rename = "globalTypes",
        skip_serializing_if = "GlobalTypesConfig::is_empty"
    )]
    pub global_types: GlobalTypesConfig,
}

/// Feature flags parsed from config keys that are not exposed as stable Rust
/// model fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigFeatureFlags {
    /// Resolve Vue 3 Options API template bindings during type checking.
    /// Default-on (matches vue-tsc): an Options API SFC's template bindings
    /// (`data`/`computed`/`methods`/`props`) resolve without configuration.
    /// Set `typeChecker.optionsApi: false` to opt out. Available in the standard
    /// build (not a legacy feature).
    pub type_checker_options_api: bool,
    pub type_checker_legacy_vue2: bool,
    /// Opt-in type-checking of `.jsx`/`.tsx` Vue components (#1497). Default-off
    /// so mixed Vue/React repositories do not accidentally route React `.tsx`
    /// through the Vue JSX checker. Set `typeChecker.jsxTypecheck: true` or
    /// opt into `experimentals.jsxVapor` to route `.jsx`/`.tsx` through the
    /// Vize JSX virtual-TS path instead of the verbatim passthrough.
    pub type_checker_jsx_typecheck: bool,
    pub language_server_legacy_vue2: Option<bool>,
    /// Dialect selected by `vue.version`; `None` when the key is absent
    /// (modern Vue 3). Validated at parse time — unknown or ambiguous values
    /// fail config loading instead of silently picking a line. Groundwork for
    /// legacy Vue support (#1392): consumers thread this into parser and
    /// transform options in follow-ups.
    pub vue_version: Option<VueVersion>,
    /// Default JSX/TSX output backend selected by `compiler.jsxMode` (#1496);
    /// `None` when the key is absent (treated as VDOM). The JS plugins and the
    /// native `compileJsx` binding thread this into the per-component
    /// mode-selection logic so a single module can still mix VDOM and Vapor via
    /// `"use vue:*"` directives.
    pub jsx_mode: Option<JsxMode>,
    /// JSX/TSX compatibility semantics selected by `compiler.jsxCompat` (#3391);
    /// `None` when the key is absent (treated as `native`). Opting into `babel`
    /// asks the JSX compiler for `@vue/babel-plugin-jsx` semantics instead of
    /// Vize's own; the JS plugins and the native `compileJsx` binding thread it
    /// through the same way as `jsx_mode`.
    pub jsx_compat: Option<JsxCompat>,
    pub experimental_vapor: bool,
    pub experimental_jsx_vapor: bool,
    pub experimental_in_tag_comments: bool,
    pub experimental_patterned_template: bool,
    pub experimental_server_script: bool,
}

impl Default for ConfigFeatureFlags {
    fn default() -> Self {
        Self {
            // Options API resolution is default-on (matches vue-tsc).
            type_checker_options_api: true,
            type_checker_legacy_vue2: false,
            type_checker_jsx_typecheck: false,
            language_server_legacy_vue2: None,
            vue_version: None,
            jsx_mode: None,
            jsx_compat: None,
            experimental_vapor: false,
            experimental_jsx_vapor: false,
            experimental_in_tag_comments: false,
            experimental_patterned_template: false,
            experimental_server_script: false,
        }
    }
}

/// Lint-only feature switches derived from config compatibility keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinterFeatureFlags {
    pub vue_version: Option<VueVersion>,
    pub vapor: Option<bool>,
}

impl LinterFeatureFlags {
    pub(crate) fn from_config_features(
        features: ConfigFeatureFlags,
        compiler_compatibility_vue_version: Option<VueVersion>,
        compiler_vapor: Option<bool>,
    ) -> Self {
        let vue_version = features
            .vue_version
            .or(compiler_compatibility_vue_version)
            .or_else(|| {
                (features.type_checker_legacy_vue2
                    || features.language_server_legacy_vue2 == Some(true))
                .then_some(VueVersion::V2_7)
            });
        Self {
            vue_version,
            vapor: compiler_vapor,
        }
    }
}

/// Raw config representation with legacy aliases preserved for migration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct RawVizeConfig {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,
    #[serde(rename = "basePath")]
    pub base_path: Option<String>,
    pub files: Option<Vec<String>>,
    pub dialect: Option<VueDialect>,
    pub formatter: FormatterConfig,
    pub(crate) compiler: RawCompilerConfig,
    pub(crate) compatibility: compatibility::RawCompatibilityConfig,
    pub(crate) experimentals: RawExperimentalsConfig,
    pub(crate) vue: RawVueConfig,
    pub linter: RawLinterConfig,
    #[serde(rename = "typeChecker")]
    type_checker: RawTypeCheckerConfig,
    #[serde(rename = "languageServer")]
    language_server: RawLanguageServerConfig,
    #[serde(rename = "globalTypes")]
    pub global_types: RawGlobalTypesConfig,
    pub ignores: Option<Vec<String>>,
    pub entries: Option<Vec<RawConfigEntry>>,
    #[serde(rename = "check")]
    legacy_check: Option<LegacyCheckConfig>,
    #[serde(rename = "fmt")]
    legacy_formatter: Option<FormatterConfig>,
    #[serde(rename = "lsp")]
    legacy_lsp: Option<RawLanguageServerConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawTypeCheckerConfig {
    #[serde(flatten)]
    config: TypeCheckerConfig,
    /// `None` when `typeChecker.optionsApi` is absent — defaults to enabled
    /// (matches vue-tsc). Set `false` to opt out.
    options_api: Option<bool>,
    legacy_vue2: bool,
    /// Opt-in type-checking of JSX/TSX Vue components (#1497). Default-off.
    jsx_typecheck: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawLanguageServerConfig {
    #[serde(flatten)]
    config: LanguageServerConfig,
    legacy_vue2: Option<bool>,
    signature_help: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct LegacyCheckConfig {
    globals: Option<String>,
    servers: Option<usize>,
}

impl RawVizeConfig {
    /// Read the language server switches that are not stable model fields.
    ///
    /// Resolves the same `languageServer` / legacy `lsp` precedence as
    /// [`Self::into_config_and_features`] without consuming the raw config.
    pub(crate) fn language_server_unstable_flags(&self) -> LanguageServerUnstableFlags {
        let raw = match self.legacy_lsp.as_ref() {
            Some(legacy) if self.language_server.config == LanguageServerConfig::default() => {
                legacy
            }
            _ => &self.language_server,
        };
        LanguageServerUnstableFlags {
            signature_help: raw.signature_help,
        }
    }

    /// Normalize raw config and derive auxiliary feature flags once.
    ///
    /// Legacy aliases (`check`, `fmt`, `lsp`) are folded here while the raw
    /// object is still owned. Callers that also need linter settings clone them
    /// before this conversion, which avoids a second deserialization pass.
    pub(crate) fn into_config_and_features(self) -> (VizeConfig, ConfigFeatureFlags) {
        let RawVizeConfig {
            schema,
            base_path: _,
            files: _,
            dialect,
            formatter,
            compiler,
            compatibility,
            experimentals,
            vue,
            linter: _,
            type_checker: raw_type_checker,
            language_server: raw_language_server,
            global_types,
            ignores: _,
            entries: _,
            legacy_check,
            legacy_formatter,
            legacy_lsp,
        } = self;

        // Default-on (matches vue-tsc); explicit `false` opts out.
        let type_checker_options_api = raw_type_checker.options_api.unwrap_or(true);
        let vue_version = vue
            .version
            .or(compiler.compatibility.vue_version)
            .or(compatibility.vue_version);
        // A Vue 2 / 2.7 dialect implies legacy template lowering. Every consumer
        // downstream of the virtual-TS generator already collapses the two into
        // `legacy_vue2 || dialect ∈ {V2, V2_7}`, but the flag itself gates
        // slot-scope and filter lowering, so without this fold `vize check` and
        // `vize lsp` both reported "Cannot find name" on pristine Vue 2.7 files
        // unless `typeChecker.legacyVue2` was *also* set (#3297). This is the
        // mirror of `LinterFeatureFlags::from_config_features`, which derives
        // the dialect from the legacy flags.
        let type_checker_legacy_vue2 = raw_type_checker.legacy_vue2
            || matches!(vue_version, Some(VueVersion::V2 | VueVersion::V2_7));
        let experimental_jsx_vapor = experimentals.jsx_vapor_enabled();
        let type_checker_jsx_typecheck = raw_type_checker.jsx_typecheck || experimental_jsx_vapor;
        let mut type_checker = raw_type_checker.config;
        if let Some(legacy_check) = legacy_check {
            if type_checker.globals_file.is_none() {
                type_checker.globals_file = legacy_check.globals;
            }
            if type_checker.servers.is_none() {
                type_checker.servers = legacy_check.servers;
            }
        }

        let formatter = if formatter == FormatterConfig::default() {
            legacy_formatter.unwrap_or(formatter)
        } else {
            formatter
        };

        let language_server_raw = if raw_language_server.config == LanguageServerConfig::default() {
            legacy_lsp.unwrap_or(raw_language_server)
        } else {
            raw_language_server
        };
        let language_server = language_server_raw.config;
        let features = ConfigFeatureFlags {
            type_checker_options_api,
            type_checker_legacy_vue2,
            type_checker_jsx_typecheck,
            language_server_legacy_vue2: language_server_raw.legacy_vue2,
            vue_version,
            jsx_mode: compiler
                .jsx_mode
                .or_else(|| experimental_jsx_vapor.then_some(JsxMode::Vapor)),
            jsx_compat: compiler.jsx_compat,
            experimental_vapor: experimentals.vapor_enabled(),
            experimental_jsx_vapor,
            experimental_in_tag_comments: experimentals.in_tag_comments_enabled(),
            experimental_patterned_template: experimentals.patterned_template_enabled(),
            experimental_server_script: experimentals.server_script_enabled(),
        };

        let config = VizeConfig {
            schema,
            dialect,
            formatter,
            type_checker,
            language_server,
            global_types: global_types.into(),
        };

        (config, features)
    }
}

impl From<RawVizeConfig> for VizeConfig {
    fn from(raw: RawVizeConfig) -> Self {
        raw.into_config_and_features().0
    }
}
